use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde_json::{Value, json};
use std::{fs::File, io::Read};
use trady_adapters::{
    credentials::CredentialStore,
    financial::{FinancialClient, offline_page},
};
use trady_application::financial::*;
use trady_domain::{Error, Result, financial::*, require};
use trady_storage::financial::PgFinancialRepository;

pub const HELP: &str = "\n  trady financial status [--config FILE]\n  trady financial collect --once [--source ID] [--start YYYY-MM-DD] [--end YYYY-MM-DD] [--config FILE]\n  trady financial snapshot|signals|features --source ID [--as-of RFC3339] [--consumer DISPLAY|EXTERNAL_LLM|LOCAL_INFERENCE|MODEL_TRAINING|EXPORT] [--limit 1..500] [--config FILE]\n  trady financial import FILE --source ID [--config FILE]\n  trady financial health|purge [--config FILE]\n\nFinancial data are research context. Credentials resolve centrally; sources ship disabled. Collection uses TRADY_ANALYST_DSN, weighted account budgets and reviewed capabilities/rights. Historical prices cannot supply execution quotes.\n";

fn bounded(path: &str) -> Result<Vec<u8>> {
    let mut bytes = vec![];
    File::open(path)
        .map_err(|_| Error("financial input unavailable".into()))?
        .take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Error("financial input unreadable".into()))?;
    require(
        bytes.len() <= 2 * 1024 * 1024,
        "financial input exceeds bound",
    )?;
    Ok(bytes)
}
pub fn run(args: &[String]) -> Result<Value> {
    let command = args
        .first()
        .map(String::as_str)
        .ok_or_else(|| Error(HELP.into()))?;
    require(
        [
            "status", "collect", "snapshot", "signals", "features", "import", "health", "purge",
        ]
        .contains(&command),
        "unsupported financial command",
    )?;
    let mut file = None;
    let mut source_id = None;
    let mut as_of = None;
    let mut start = None;
    let mut end = None;
    let mut consumer = if command == "features" {
        Consumer::LocalInference
    } else {
        Consumer::Display
    };
    let mut limit = 500;
    let mut once = false;
    let mut import = None;
    let mut seen = std::collections::BTreeSet::new();
    let mut i = 1;
    while i < args.len() {
        let flag = args[i].as_str();
        if command == "import" && i == 1 && !flag.starts_with('-') {
            import = Some(flag.to_owned());
            i += 1;
            continue;
        }
        require(seen.insert(flag.to_owned()), "duplicate financial argument")?;
        if flag == "--once" {
            once = true;
            i += 1;
            continue;
        }
        let value = args
            .get(i + 1)
            .ok_or_else(|| Error("financial argument value missing".into()))?;
        match flag {
            "--config" => file = Some(value.clone()),
            "--source" => source_id = Some(value.clone()),
            "--as-of" => {
                as_of = Some(
                    DateTime::parse_from_rfc3339(value)
                        .map_err(|_| Error("invalid financial cutoff".into()))?
                        .with_timezone(&Utc),
                )
            }
            "--start" => {
                start = Some(
                    NaiveDate::parse_from_str(value, "%Y-%m-%d")
                        .map_err(|_| Error("invalid financial start".into()))?,
                )
            }
            "--end" => {
                end = Some(
                    NaiveDate::parse_from_str(value, "%Y-%m-%d")
                        .map_err(|_| Error("invalid financial end".into()))?,
                )
            }
            "--limit" => {
                limit = value
                    .parse::<usize>()
                    .ok()
                    .filter(|n| (1..=500).contains(n))
                    .ok_or_else(|| Error("invalid financial limit".into()))?
            }
            "--consumer" => {
                consumer = serde_json::from_value(Value::String(value.clone()))
                    .map_err(|_| Error("invalid financial consumer".into()))?
            }
            _ => return Err(Error("unknown financial argument".into())),
        }
        i += 2;
    }
    require(
        once == (command == "collect")
            && (as_of.is_none() || matches!(command, "snapshot" | "signals" | "features"))
            && (start.is_none() && end.is_none() || command == "collect"),
        "financial flags do not apply to this command",
    )?;
    let allowed: &[&str] = match command {
        "status" => &["--config", "--source"],
        "collect" => &["--config", "--source", "--start", "--end", "--once"],
        "snapshot" | "signals" | "features" => {
            &["--config", "--source", "--as-of", "--consumer", "--limit"]
        }
        "import" => &["--config", "--source"],
        _ => &["--config"],
    };
    require(
        seen.iter().all(|f| allowed.contains(&f.as_str()))
            && (!matches!(command, "snapshot" | "signals" | "features" | "import")
                || source_id.is_some())
            && (command != "import" || import.is_some()),
        "missing or inapplicable financial command arguments",
    )?;
    let bytes = if let Some(path) = file {
        bounded(&path)?
    } else {
        include_bytes!("../../../config/financial-data.json").to_vec()
    };
    let config: FinancialConfig =
        serde_json::from_slice(&bytes).map_err(|_| Error("invalid financial registry".into()))?;
    config.validate()?;
    if let Some(id) = &source_id {
        require(
            config.sources.iter().any(|s| &s.id == id),
            "unknown financial source",
        )?;
    }
    let now = Utc::now();
    if command == "status" {
        let store = CredentialStore::load()?;
        let mut sources = vec![];
        for source in config
            .sources
            .iter()
            .filter(|s| source_id.as_ref().is_none_or(|id| id == &s.id))
        {
            let account = config.account(source)?;
            let provider = source.request.provider();
            let configured = if let Some(name) = provider.credential_name() {
                store.optional(name)?.is_some()
            } else {
                true
            };
            sources.push(json!({"source_id":source.id,"provider":provider,"endpoint":source.request,"entity_id":source.entity_id,
                "enabled":source.enabled,"account_enabled":account.enabled,"capability":source.capability.status,"capability_current":source.capability.current(now),
                "credential_name":provider.credential_name(),"credential_configured":configured,"collection_ready":source.usable(now)&&account.enabled&&configured&&provider!=Provider::Yfinance,
                "rights":source.rights,"limits":account.limits}));
        }
        return Ok(
            json!({"config_version":1,"research_only":true,"sources":sources,"entry_authorized":false}),
        );
    }
    let mut repo = PgFinancialRepository::connect(&super::secret("TRADY_ANALYST_DSN")?)?;
    repo.sync_registry(&config)?;
    match command {
        "health" => repo.health(),
        "purge" => {
            repo.purge(now)?;
            Ok(json!({"status":"purged_expired_or_revoked_payloads","entry_authorized":false}))
        }
        "collect" => {
            let window = DataWindow {
                start: start.unwrap_or(now.date_naive() - Duration::days(366)),
                end: end.unwrap_or(now.date_naive() - Duration::days(1)),
            };
            window.validate(now)?;
            let mut client = FinancialClient::default();
            let mut reports = vec![];
            for source in config
                .sources
                .iter()
                .filter(|s| source_id.as_ref().is_none_or(|id| id == &s.id))
            {
                if source.request == DataRequest::YfinanceImport {
                    continue;
                }
                reports.push(collect(
                    source,
                    config.account(source)?,
                    &window,
                    &mut client,
                    &mut repo,
                    Utc::now(),
                )?);
            }
            Ok(json!({"reports":reports,"entry_authorized":false}))
        }
        "snapshot" | "signals" | "features" => {
            let id = source_id.ok_or_else(|| Error("financial read requires --source".into()))?;
            let snapshot = repo.read(&id, as_of.unwrap_or(now), consumer, limit, now)?;
            if command == "features" {
                serde_json::to_value(model_features(&signals(snapshot)?)?)
            } else if command == "signals" {
                serde_json::to_value(signals(snapshot)?)
            } else {
                serde_json::to_value(snapshot)
            }
            .map_err(|_| Error("financial output encoding failed".into()))
        }
        "import" => {
            let id = source_id.ok_or_else(|| Error("financial import requires --source".into()))?;
            let source = config.sources.iter().find(|s| s.id == id).unwrap();
            let page = offline_page(
                source,
                now,
                &bounded(&import.ok_or_else(|| Error("financial import file required".into()))?)?,
            )?;
            struct Offline(FinancialPage);
            impl FinancialProvider for Offline {
                fn fetch(
                    &mut self,
                    _: &FinancialSource,
                    _: &DataWindow,
                    _: DateTime<Utc>,
                ) -> std::result::Result<FinancialPage, FinancialFailure> {
                    Ok(self.0.clone())
                }
            }
            let window = DataWindow {
                start: now.date_naive() - Duration::days(366),
                end: now.date_naive() - Duration::days(1),
            };
            Ok(
                json!({"report":collect(source,config.account(source)?,&window,&mut Offline(page),&mut repo,now)?,"entry_authorized":false}),
            )
        }
        _ => Err(Error("unsupported financial command".into())),
    }
}

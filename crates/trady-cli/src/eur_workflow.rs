//! Reporting and scheduling use recorded execution facts. Handoffs contain a
//! strict allowlist and remain PENDING_REVIEW; this module cannot approve them.
use chrono::{Datelike, Duration, Utc};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};
use trady_application::{eur_runtime::EurRuntimeConfig, harness::content_hash};
use trady_domain::{Error, ExecutionStage, Result, require};
use trady_storage::target::PgTargetStore;

pub fn run(action: &str, args: &[String]) -> Result<Value> {
    require(
        args.len() == 2 || args.len() == 4,
        "usage: trady eur weekly|handoff --config FILE [--out FILE]",
    )?;
    require(
        args[0] == "--config" && (args.len() == 2 || args[2] == "--out"),
        "invalid review workflow arguments",
    )?;
    let config: EurRuntimeConfig = super::eur::read_json(Path::new(&args[1]))?;
    config.validate(Utc::now())?;
    require(
        matches!(
            config.execution_stage,
            ExecutionStage::Shadow | ExecutionStage::Demo
        ),
        "review workflow requires the demo EUR account namespace",
    )?;
    let mut store = PgTargetStore::connect(
        &super::secret("TRADY_ANALYST_DSN")?,
        "trady_analyst",
        config.owner_id.clone(),
        config.account_id.to_string(),
    )?;
    let now = Utc::now();
    let day = now.date_naive();
    let monday = day - Duration::days(i64::from(day.weekday().num_days_from_monday()));
    let start = if action == "weekly" { monday } else { day };
    let since = start
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| Error("review period invalid".into()))?
        .and_utc();
    let rows = store.list_documents("daily_report", since, 1000)?;
    let rows = rows
        .as_array()
        .ok_or_else(|| Error("daily report history unavailable".into()))?;
    require(
        !rows.is_empty(),
        "no reconciled daily reports are available for a review handoff",
    )?;
    let mut by_day = BTreeMap::new();
    let mut observed_runs = 0u64;
    for row in rows {
        let doc = &row["document"];
        if let Some(timestamp) = doc["completed_at"]
            .as_str()
            .and_then(|v| v.parse::<chrono::DateTime<Utc>>().ok())
        {
            if timestamp < since || timestamp > now {
                continue;
            }
            observed_runs += 1;
            let key = timestamp.date_naive();
            if by_day.get(&key).is_none_or(|old: &&Value| {
                old["document"]["completed_at"]
                    .as_str()
                    .and_then(|v| v.parse::<chrono::DateTime<Utc>>().ok())
                    .is_none_or(|old| old < timestamp)
            }) {
                by_day.insert(key, row);
            }
        }
    }
    require(!by_day.is_empty(), "daily report timestamps missing")?;
    let reports: Vec<_> = by_day.values().map(|r| r["record_id"].clone()).collect();
    let mut evidence = BTreeSet::new();
    let mut hashes = BTreeMap::new();
    let mut selected = 0u64;
    let mut flat = 0u64;
    let mut mutations = 0u64;
    let mut economic_reconciliation_complete = true;
    let mut net_pnl_microuro = Some(0i64);
    for row in by_day.values() {
        let doc = &row["document"];
        if doc["selected_instrument_id"].is_string() {
            selected += 1;
        }
        if doc["reconciled_flat"] == true {
            flat += 1;
        }
        mutations += doc["broker_mutations"].as_u64().unwrap_or(0);
        economic_reconciliation_complete &= doc["economic_reconciliation_complete"] == true;
        net_pnl_microuro = net_pnl_microuro.and_then(|sum| {
            doc["realized_pnl_microuro"]
                .as_i64()
                .and_then(|amount| sum.checked_add(amount))
        });
        for id in doc["provider_observation_ids"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            evidence.insert(id.to_owned());
        }
        hashes.insert(
            format!("report_{}", hashes.len()),
            row["content_sha256"].clone(),
        );
    }
    let latest = by_day.values().last().unwrap()["document"].clone();
    let prior_history_lost = store
        .read_document("recovery", "volatile_db_recovery_20261104")?
        .is_some_and(|record| record["unarchived_history_restored"] == false);
    let mut expected_days = None;
    if let Some(settings) = config.shadow.as_ref() {
        let crosswalk_value: Value = super::eur::read_json(&super::eur_shadow::resolve(
            Path::new(&args[1]),
            &settings.crosswalk_config,
        ))?;
        let crosswalk: trady_application::eur_shadow::ShadowCrosswalk =
            serde_json::from_value(crosswalk_value.clone())
                .map_err(|_| Error("review universe reference invalid".into()))?;
        if content_hash(&crosswalk_value.to_string())
            == latest["crosswalk_hash"].as_str().unwrap_or_default()
            && let Some(id) = latest["provider_observation_ids"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .find(|id| id.ends_with("_broker_sessions"))
            && let Some(value) = store.read_document("provider_snapshot", id)?
        {
            let observation: trady_application::eur_shadow::ProviderObservation =
                serde_json::from_value(value)
                    .map_err(|_| Error("review calendar observation invalid".into()))?;
            observation.validate()?;
            if observation.status == trady_application::eur_shadow::ObservationStatus::Ok {
                let mut days = BTreeSet::new();
                let mut verified = true;
                for identity in &crosswalk.members {
                    let schedule = observation
                        .response
                        .as_array()
                        .into_iter()
                        .flatten()
                        .find(|v| v["id"].as_i64() == Some(identity.broker_exchange_id))
                        .and_then(|v| v["workingSchedules"].as_array())
                        .into_iter()
                        .flatten()
                        .find(|s| s["id"].as_i64() == Some(identity.working_schedule_id));
                    if let Some(schedule) = schedule {
                        // Absence of an OPEN event is not evidence of a holiday
                        // or of full-week calendar coverage.
                        verified &= full_week_calendar_covered(schedule, monday);
                        for event in schedule["timeEvents"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter(|e| e["type"] == "OPEN")
                        {
                            if let Some(at) = event["date"]
                                .as_str()
                                .and_then(|v| v.parse::<chrono::DateTime<Utc>>().ok())
                                && at.date_naive() >= monday
                                && at.date_naive() < monday + Duration::days(5)
                            {
                                days.insert(at.date_naive());
                            }
                        }
                    } else {
                        verified = false;
                    }
                }
                if verified {
                    expected_days = Some(days);
                    hashes.insert("session_calendar".into(), json!(observation.response_hash));
                    hashes.insert("universe".into(), latest["crosswalk_hash"].clone());
                }
            }
        }
    }
    let code_hash = content_hash(include_str!("eur_workflow.rs"));
    let period_closed = action != "weekly"
        || now >= monday.and_hms_opt(0, 0, 0).unwrap().and_utc() + Duration::days(5);
    let complete_coverage = period_closed
        && action == "weekly"
        && expected_days
            .as_ref()
            .is_some_and(|days| days.iter().all(|day| by_day.contains_key(day)));
    let native_month_reports = store.list_documents("demo_month_report", since, 1000)?;
    let latest_cutoff = latest["completed_at"]
        .as_str()
        .and_then(|v| v.parse::<chrono::DateTime<Utc>>().ok());
    let native_month = native_month_reports
        .as_array()
        .into_iter()
        .flatten()
        .rev()
        .find(|r| {
            r["document"]["cutoff"]
                .as_str()
                .and_then(|v| v.parse::<chrono::DateTime<Utc>>().ok())
                .zip(latest_cutoff)
                .is_some_and(|(cutoff, end)| cutoff >= since && cutoff <= end)
        });
    if let Some(report) = native_month {
        hashes.insert(
            "trusted_demo_month_report".into(),
            report["content_sha256"].clone(),
        );
    }
    let id = format!(
        "review_{}",
        &content_hash(
            &json!([
                action,
                start,
                reports,
                hashes,
                code_hash,
                period_closed,
                expected_days,
                prior_history_lost
            ])
            .to_string()
        )[..40]
    );
    let mut bundle = json!({"schema_version":1,"bundle_id":id,"status":"PENDING_REVIEW","report_ids":reports,
        "evidence_ids":evidence.into_iter().take(256).collect::<Vec<_>>(),"hashes":hashes,
        "metrics":{"observed_runs":observed_runs,"observed_trading_days":by_day.len(),"selected_days":selected,
            "flat_days":flat,"broker_mutations_observed":mutations,"period_closed":period_closed,
            "complete_week_coverage":complete_coverage,"calendar_coverage_verified":expected_days.is_some(),
            "expected_trading_days":expected_days.as_ref().map_or(0,BTreeSet::len),
            "trusted_native_month_report_present":native_month.is_some(),
            "native_month_fill_count":native_month.and_then(|r|r["document"]["native_fill_count"].as_u64()).unwrap_or(0),
            "native_month_economic_reconciliation_complete":native_month.is_some_and(|r|r["document"]["economic_reconciliation_complete"]==true),
            "performance_metrics_available":economic_reconciliation_complete && complete_coverage && net_pnl_microuro.is_some(),
            "prior_unarchived_database_history_lost":prior_history_lost},
        "review_questions":["Which recorded blockers prevented a valid selection?","Do source errors or identity changes need investigation?",
            "What offline evidence is still required before changing policy?"],
        "policy_revision":config.policy.revision,"configuration_hash":latest["configuration_hash"],
        "period_start":since,"period_end":latest["completed_at"],"generated_at":latest["completed_at"],"code_hash":code_hash});
    if bundle["metrics"]["performance_metrics_available"] == true {
        bundle["metrics"]["realized_pnl_microuro"] = json!(net_pnl_microuro);
    }
    validate_handoff(&bundle)?;
    let value = if let Some(old) = store.read_review_artifact(&id)? {
        require(old == bundle, "review bundle rebound")?;
        old
    } else {
        store.append_review_artifact(&id, &bundle)?;
        bundle
    };
    let out = args
        .get(3)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| Path::new("/var/lib/trady-analyst/reviews").join(format!("{id}.json")));
    if let Some(parent) = out.parent()
        && !parent.exists()
    {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder
            .create(parent)
            .map_err(|_| Error("review output directory unavailable".into()))?;
    }
    if out.exists() {
        let old: Value = super::eur::read_json(&out)?;
        require(old == value, "immutable review output differs")?;
    } else {
        super::eur_shadow::private_json(&out, &value)?;
    }
    Ok(
        json!({"status":"PENDING_REVIEW","bundle_id":id,"artifact":out,"code_hash":code_hash,
        "reports":by_day.len(),"activation_authority":false,"broker_mutations":0}),
    )
}
fn full_week_calendar_covered(schedule: &Value, monday: chrono::NaiveDate) -> bool {
    let mut events = BTreeMap::<_, (Vec<_>, Vec<_>)>::new();
    for event in schedule["timeEvents"].as_array().into_iter().flatten() {
        if let Some(at) = event["date"]
            .as_str()
            .and_then(|v| v.parse::<chrono::DateTime<Utc>>().ok())
            && at.date_naive() >= monday
            && at.date_naive() < monday + Duration::days(5)
        {
            let row = events.entry(at.date_naive()).or_default();
            match event["type"].as_str() {
                Some("OPEN") => row.0.push(at),
                Some("CLOSE") => row.1.push(at),
                _ => {}
            }
        }
    }
    (0..5).all(|n| {
        events
            .get(&(monday + Duration::days(n)))
            .is_some_and(|(opens, closes)| {
                opens.len() == 1 && closes.len() == 1 && opens[0] < closes[0]
            })
    })
}
pub fn validate_handoff(value: &Value) -> Result<()> {
    let allowed = [
        "schema_version",
        "bundle_id",
        "status",
        "report_ids",
        "evidence_ids",
        "hashes",
        "metrics",
        "review_questions",
        "policy_revision",
        "configuration_hash",
        "period_start",
        "period_end",
        "generated_at",
        "code_hash",
    ];
    let object = value
        .as_object()
        .ok_or_else(|| Error("review handoff is not an object".into()))?;
    require(
        value["status"] == "PENDING_REVIEW"
            && object.keys().all(|key| allowed.contains(&key.as_str()))
            && value["report_ids"]
                .as_array()
                .is_some_and(|a| !a.is_empty() && a.len() <= 256)
            && value["evidence_ids"]
                .as_array()
                .is_some_and(|a| a.len() <= 256)
            && value["metrics"]
                .as_object()
                .is_some_and(|m| m.values().all(|v| v.is_number() || v.is_boolean())),
        "review handoff contains unapproved fields or state",
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn secrets_mutation_tools_and_self_approval_are_rejected() {
        let mut bundle = json!({"status":"PENDING_REVIEW","report_ids":["report"],"evidence_ids":[],"metrics":{"count":1}});
        assert!(validate_handoff(&bundle).is_ok());
        bundle["credentials"] = json!({"password":"fixture"});
        assert!(validate_handoff(&bundle).is_err());
        bundle.as_object_mut().unwrap().remove("credentials");
        bundle["status"] = json!("APPROVED");
        assert!(validate_handoff(&bundle).is_err());
        bundle["status"] = json!("PENDING_REVIEW");
        bundle["mutation_tools"] = json!(["broker.submit"]);
        assert!(validate_handoff(&bundle).is_err());
    }
    #[test]
    fn missing_calendar_days_cannot_be_invented_as_verified_holidays() {
        let monday = "2026-11-02".parse::<chrono::NaiveDate>().unwrap();
        let mut events = Vec::new();
        for n in 0..5 {
            let day = monday + Duration::days(n);
            events.push(json!({"type":"OPEN","date":format!("{day}T08:00:00Z")}));
            events.push(json!({"type":"CLOSE","date":format!("{day}T16:30:00Z")}));
        }
        assert!(full_week_calendar_covered(
            &json!({"timeEvents":events}),
            monday
        ));
        let mut missing = events.clone();
        missing.drain(0..2);
        assert!(!full_week_calendar_covered(
            &json!({"timeEvents":missing}),
            monday
        ));
        let mut ambiguous = events.clone();
        ambiguous.push(events[0].clone());
        assert!(!full_week_calendar_covered(
            &json!({"timeEvents":ambiguous}),
            monday
        ));
    }
}

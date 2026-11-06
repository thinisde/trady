//! Exact reviewed capability attribution. Never an entry/protocol acceptance.
use super::eur_execute::{file_hash, read};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
use trady_application::eur_protocol::{
    OrderProgress, ProtocolContext, ProtocolRecord, ProtocolState, recover, state_hash,
};
use trady_domain::{Error, Result, require};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Registry {
    pub schema_version: u32,
    pub kind: String,
    pub owner_id: String,
    pub account_id: String,
    pub broker_account_id: u64,
    pub environment: String,
    pub live_enabled: bool,
    pub passed: bool,
    pub reviewer: Option<String>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub valid_until: DateTime<Utc>,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub evidence_sha256: String,
    pub activities: Vec<Activity>,
    pub external_cleanups: Vec<ExternalCleanup>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Activity {
    pub scope_name: String,
    pub account_id: String,
    pub intent_id: String,
    pub order_ids: Vec<String>,
    pub fill_ids: Vec<String>,
    pub flat_at: DateTime<Utc>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ExternalCleanup {
    pub scope_name: String,
    pub owner_id: String,
    pub account_id: String,
    pub intent_id: String,
    pub head_revision: u64,
    pub state_sha256: String,
    pub order_ids: Vec<String>,
    pub fill_ids: Vec<String>,
}
#[derive(Debug, Clone, Default)]
pub(super) struct CapabilityAttribution {
    pub fills: BTreeMap<String, Value>,
    pub opening_accounts: Vec<Value>,
}
pub(super) fn decimal(v: &Value) -> Result<Decimal> {
    v.as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| v.to_string())
        .parse()
        .map_err(|_| Error("native attribution amount missing".into()))
}
fn clock(v: &Value) -> Result<DateTime<Utc>> {
    serde_json::from_value(v.clone()).map_err(|_| Error("native attribution clock missing".into()))
}
fn ids(values: &[String]) -> Result<BTreeSet<String>> {
    let ids: BTreeSet<_> = values.iter().cloned().collect();
    require(
        ids.len() == values.len()
            && !ids.is_empty()
            && ids
                .iter()
                .all(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit())),
        "invalid or duplicate native attribution IDs",
    )?;
    Ok(ids)
}
fn native_rows(evidence: &Value) -> Result<(BTreeMap<String, Value>, BTreeMap<String, Value>)> {
    let history = &evidence["native_history"];
    let pages = history["pages"]
        .as_array()
        .ok_or_else(|| Error("native history pages missing".into()))?;
    require(
        history["complete"] == true
            && history["classification"] == "BOUNDED_READ_ONLY_ACCOUNT_WIDE_NATIVE_HISTORY"
            && history["broker_mutations"] == 0
            && history["entry_authorized"] == false
            && !pages.is_empty()
            && pages.len() <= 8
            && history["page_count"].as_u64() == Some(pages.len() as u64),
        "capability native history incomplete",
    )?;
    let mut orders = BTreeMap::new();
    let mut fills = BTreeMap::new();
    let mut cursor = None;
    let mut seen = BTreeSet::new();
    for (index, page) in pages.iter().enumerate() {
        require(
            page["provider"] == "trading212_demo"
                && page["http_status"] == 200
                && page["endpoint"] == "https://demo.trading212.com/api/v0/equity/history/orders"
                && page["parameters"]["limit"] == 50
                && page["parameters"].as_object().is_some_and(|p| {
                    p.len() == if index == 0 { 1 } else { 2 }
                        && p.keys()
                            .all(|k| k == "limit" || (index > 0 && k == "cursor"))
                })
                && page["parameters"].get("cursor").and_then(Value::as_str) == cursor.as_deref()
                && clock(&page["requested_at"])? <= clock(&page["received_at"])?
                && page["broker_mutations"] == 0,
            "capability native history provider changed",
        )?;
        for row in page["response"]["items"]
            .as_array()
            .ok_or_else(|| Error("native history rows missing".into()))?
        {
            let id = row["order"]["id"]
                .as_u64()
                .filter(|id| *id > 0)
                .ok_or_else(|| Error("native order ID missing".into()))?
                .to_string();
            if let Some(old) = orders.insert(id, row["order"].clone()) {
                require(old == row["order"], "conflicting native order history")?;
            }
            if !row["fill"].is_null() {
                clock(&row["fill"]["filledAt"])?;
                let id = row["fill"]["id"]
                    .as_u64()
                    .filter(|id| *id > 0)
                    .ok_or_else(|| Error("native fill ID missing".into()))?
                    .to_string();
                if let Some(old) = fills.insert(id, row.clone()) {
                    require(old == *row, "conflicting native fill history")?;
                }
            }
        }
        cursor = match page["response"]["nextPagePath"].as_str() {
            Some(path) => {
                let query = path
                    .strip_prefix("/api/v0/equity/history/orders?cursor=")
                    .and_then(|v| v.strip_suffix("&limit=50"))
                    .filter(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()))
                    .ok_or_else(|| Error("native history cursor shape changed".into()))?;
                require(
                    index + 1 < pages.len() && seen.insert(query.to_string()),
                    "native history end missing or cursor repeated",
                )?;
                Some(query.to_string())
            }
            None => {
                require(
                    page["response"].get("nextPagePath") == Some(&Value::Null)
                        && index + 1 == pages.len(),
                    "native history premature or absent end marker",
                )?;
                None
            }
        };
    }
    Ok((orders, fills))
}

// Evidence is self-contained: no mutable workspace paths are followed by a
// bundled verifier. Every projection is checked against retained exact bytes.
fn source_projections(evidence: &Value) -> Result<()> {
    let hashes = evidence["source_files"]
        .as_object()
        .ok_or_else(|| Error("source hashes missing".into()))?;
    let bytes = evidence["source_bytes"]
        .as_object()
        .ok_or_else(|| Error("source bytes missing".into()))?;
    let documents = evidence["source_documents"]
        .as_object()
        .ok_or_else(|| Error("source projections missing".into()))?;
    let names = [
        "observations",
        "native_history",
        "external_cleanup_attestation",
        "current_flat_observation",
        "durable_heads",
    ];
    require(
        hashes.len() == names.len() && bytes.len() == names.len() && documents.len() == names.len(),
        "source coverage changed",
    )?;
    let mut paths = BTreeSet::new();
    for name in names {
        let path = documents[name]
            .as_str()
            .ok_or_else(|| Error("source projection path missing".into()))?;
        let raw = bytes
            .get(path)
            .and_then(Value::as_str)
            .ok_or_else(|| Error("retained source bytes missing".into()))?;
        let parsed: Value =
            serde_json::from_str(raw).map_err(|_| Error("retained source JSON invalid".into()))?;
        require(
            paths.insert(path)
                && hashes.get(path).and_then(Value::as_str)
                    == Some(trady_application::harness::content_hash(raw).as_str())
                && parsed == evidence[name],
            "retained source hash or projection changed",
        )?;
    }
    Ok(())
}

fn native_leg(
    progress: &OrderProgress,
    context: &ProtocolContext,
    leg: &str,
    orders: &BTreeMap<String, Value>,
    terminal: bool,
) -> Result<()> {
    let id = progress
        .broker_id
        .as_ref()
        .ok_or_else(|| Error("native leg ID missing".into()))?;
    let order = orders
        .get(id)
        .ok_or_else(|| Error("native leg missing".into()))?;
    let buy = leg == "ENTRY";
    let quantity = decimal(&order["quantity"])?;
    require(
        order["ticker"] == context.broker_ticker && order["instrument"]["isin"] == context.isin
        && order["instrument"]["currency"] == "EUR" && order["currency"] == "EUR"
        && order["side"] == (if buy {"BUY"} else {"SELL"})
        && order["type"] == (match leg {"ENTRY" => "LIMIT", "PROTECTION" => "STOP", _ => "MARKET"})
        && (if buy {quantity > Decimal::ZERO} else {quantity < Decimal::ZERO})
        && quantity.abs() == progress.quantity
        && (!terminal || (progress.reconciled_terminal && decimal(&order["filledQuantity"])?.abs() == progress.filled
             && serde_json::to_value(progress.status).map_err(|_| Error("leg status encoding failed".into()))? == order["status"]))
        && (leg != "ENTRY" || decimal(&order["limitPrice"])? == context.entry_limit)
        // Old externally cleaned journals retain their failed precision inputs;
        // only a completed native lifecycle claims exact STOP price agreement.
        && (!terminal || leg != "PROTECTION" || progress.stop_price == Some(decimal(&order["stopPrice"])?)),
        "native leg quantity/type/identity/status differs from original journal",
    )
}

fn retained_head(head: &Value) -> Result<ProtocolState> {
    let state: ProtocolState = serde_json::from_value(head["state"].clone())
        .map_err(|_| Error("durable typed state invalid".into()))?;
    state.context.validate()?;
    require(
        state.revision == head["revision"].as_u64().unwrap_or(u64::MAX)
            && state_hash(&state)? == head["state_sha256"].as_str().unwrap_or("")
            && serde_json::to_value(&state.context)
                .map_err(|_| Error("context encoding failed".into()))?
                == head["context"]
            && head["account_id"] == state.context.account_id
            && head["intent_id"] == state.context.intent_id,
        "durable state hash or scope projection invalid",
    )?;
    Ok(state)
}
fn balanced_scope(
    order_ids: &[String],
    fill_ids: &[String],
    orders: &BTreeMap<String, Value>,
    fills: &BTreeMap<String, Value>,
    context: &ProtocolContext,
) -> Result<BTreeMap<String, Value>> {
    let known = ids(order_ids)?;
    let expected = ids(fill_ids)?;
    let actual: BTreeMap<_, _> = fills
        .iter()
        .filter(|(_, r)| known.contains(&r["order"]["id"].to_string()))
        .map(|(id, r)| (id.clone(), r.clone()))
        .collect();
    require(
        actual.keys().cloned().collect::<BTreeSet<_>>() == expected,
        "capability fill coverage differs from native history",
    )?;
    let mut buy = Decimal::ZERO;
    let mut sell = Decimal::ZERO;
    for id in &known {
        let order = orders
            .get(id)
            .ok_or_else(|| Error("attributed native order missing".into()))?;
        require(
            matches!(
                order["status"].as_str(),
                Some("FILLED" | "CANCELLED" | "REJECTED")
            ) && order["instrument"]["currency"] == "EUR"
                && order["ticker"] == context.broker_ticker
                && order["instrument"]["isin"] == context.isin,
            "capability native order identity/terminal state invalid",
        )?;
        let mut total = Decimal::ZERO;
        for row in actual
            .values()
            .filter(|r| r["order"]["id"].as_u64() == id.parse::<u64>().ok())
        {
            let q = decimal(&row["fill"]["quantity"])?;
            let wallet = &row["fill"]["walletImpact"];
            require(
                wallet["currency"] == "EUR"
                    && decimal(&wallet["fxRate"])? == Decimal::ONE
                    && wallet["taxes"].is_array(),
                "capability wallet currency/FX/taxes missing",
            )?;
            match order["side"].as_str() {
                Some("BUY") if q > Decimal::ZERO => buy += q,
                Some("SELL") if q < Decimal::ZERO => sell -= q,
                _ => return Err(Error("capability fill side/sign invalid".into())),
            }
            total += q.abs();
        }
        require(
            total == decimal(&order["filledQuantity"])?.abs(),
            "capability distinct fill totals incomplete",
        )?;
    }
    require(
        buy > Decimal::ZERO && buy == sell,
        "capability quantity is not exactly flat",
    )?;
    Ok(actual)
}

pub(super) fn load(
    registry_path: &Path,
    evidence_path: &Path,
    owner: &str,
    account: &str,
    now: DateTime<Utc>,
    pending_review: bool,
) -> Result<(Registry, CapabilityAttribution)> {
    load_inner(
        registry_path,
        evidence_path,
        owner,
        account,
        now,
        pending_review,
        None,
    )
}

// Historical read-only reports can use facts accepted for their exact period
// after entry authority has expired. This path never authorizes execution.
pub(super) fn load_for_report(
    registry_path: &Path,
    evidence_path: &Path,
    owner: &str,
    account: &str,
    now: DateTime<Utc>,
    period: (DateTime<Utc>, DateTime<Utc>),
) -> Result<(Registry, CapabilityAttribution)> {
    load_inner(
        registry_path,
        evidence_path,
        owner,
        account,
        now,
        false,
        Some(period),
    )
}

fn load_inner(
    registry_path: &Path,
    evidence_path: &Path,
    owner: &str,
    account: &str,
    now: DateTime<Utc>,
    pending_review: bool,
    historical_period: Option<(DateTime<Utc>, DateTime<Utc>)>,
) -> Result<(Registry, CapabilityAttribution)> {
    let registry: Registry = read(registry_path)?;
    let evidence: Value = read(evidence_path)?;
    require(
        registry.schema_version == 1
            && registry.kind == "EUR_REVIEWED_CAPABILITY_ATTRIBUTION"
            && registry.owner_id == owner
            && registry.account_id == account
            && registry.broker_account_id == 52211463
            && registry.environment == "demo"
            && !registry.live_enabled
            && (registry.valid_until > now
                || historical_period
                    .is_some_and(|(_, end)| now >= end && registry.valid_until >= end))
            && historical_period.is_none_or(|(start, end)| {
                registry.period_start == start && registry.period_end == end
            })
            && registry.period_start < registry.period_end
            && registry.period_end <= registry.valid_until
            && (registry.period_end - registry.period_start).num_days() <= 31
            && registry.evidence_sha256 == file_hash(evidence_path)?
            && registry.activities.len() <= 30
            && registry.external_cleanups.len() <= 100,
        "capability attribution authority or evidence hash invalid",
    )?;
    if !pending_review {
        require(
            registry.passed
                && registry.reviewer.as_ref().is_some_and(|v| !v.is_empty())
                && registry
                    .reviewed_at
                    .is_some_and(|at| at <= now && at < registry.valid_until),
            "capability attribution not independently accepted",
        )?;
    }
    require(
        evidence["schema_version"] == 2
            && evidence["kind"] == "PRESERVED_NATIVE_CAPABILITY_AND_EXTERNAL_CLEANUP_PROOF"
            && evidence["broker_account_id"] == 52211463
            && evidence["environment"] == "demo"
            && evidence["live_enabled"] == false,
        "capability evidence physical account changed",
    )?;
    source_projections(&evidence)?;
    let current = &evidence["current_flat_observation"];
    let current_at = clock(&current["snapshot"]["observed_at"])?;
    require(
        current["provider"] == "trading212"
            && current["environment"] == "demo"
            && current["entry_authorized"] == false
            && current["snapshot"]["account_id"] == account
            && current["snapshot"]["mode"] == "paper"
            && current["snapshot"]["orders"] == json!([])
            && current["snapshot"]["positions"] == json!([])
            && current["snapshot"]["spendable_cash"]["currency"] == "EUR"
            && decimal(&current["snapshot"]["spendable_cash"]["amount"])? >= Decimal::ZERO
            && current_at <= now
            && registry.reviewed_at.is_none_or(|at| at >= current_at),
        "retained flat observation or review clock invalid",
    )?;
    let (orders, fills) = native_rows(&evidence)?;
    for page in evidence["native_history"]["pages"].as_array().unwrap() {
        require(
            clock(&page["received_at"])? <= current_at,
            "flat observation predates native history",
        )?;
    }
    let mut result = CapabilityAttribution::default();
    let mut scopes = BTreeSet::new();
    for activity in &registry.activities {
        require(
            activity.account_id != account
                && scopes.insert((activity.account_id.clone(), activity.intent_id.clone())),
            "capability scope overlaps ordinary strategy or is duplicated",
        )?;
        let scope = evidence["observations"]["scopes"]
            .as_array()
            .and_then(|rows| rows.iter().find(|r| r["name"] == activity.scope_name))
            .ok_or_else(|| Error("exact capability experiment missing".into()))?;
        let authority = &scope["authority"];
        let raw = &scope["artifacts"]["result_exit.json"]["value"];
        require(
            authority["classification"] == "BOUNDED_DEMO_CAPABILITY_EXPERIMENT_NOT_RELEASE"
                && authority["context"]["account_id"] == activity.account_id
                && authority["context"]["intent_id"] == activity.intent_id
                && authority["context"]["broker_account_id"] == 52211463
                && authority["context"]["runtime_mode"] == "DEMO",
            "capability experiment authority differs",
        )?;
        let records: Vec<ProtocolRecord> = serde_json::from_value(raw["records"].clone())
            .map_err(|_| Error("capability protocol journal unavailable".into()))?;
        let state = recover(&records)?;
        require(
            state.phase == trady_application::eur_protocol::ProtocolPhase::Flat
                && state.holding_quantity == Decimal::ZERO
                && serde_json::to_value(&state)
                    .map_err(|_| Error("protocol encoding failed".into()))?
                    == raw["state"]
                && serde_json::to_value(&state.context)
                    .map_err(|_| Error("context encoding failed".into()))?
                    == authority["context"]
                && state.last_event_at == activity.flat_at
                && activity.flat_at <= current_at
                && state.context.created_at >= registry.period_start
                && activity.flat_at < registry.period_end,
            "capability exact journal is not terminal flat",
        )?;
        let native_ids: BTreeSet<_> = state
            .entry
            .iter()
            .chain(state.protection.iter())
            .chain(state.exits.iter())
            .filter_map(|p| p.broker_id.clone())
            .collect();
        require(
            native_ids == ids(&activity.order_ids)?,
            "capability exact order set differs from original journal",
        )?;
        for (leg, progress) in state
            .entry
            .iter()
            .map(|p| ("ENTRY", p))
            .chain(state.protection.iter().map(|p| ("PROTECTION", p)))
            .chain(state.exits.iter().map(|p| ("EXIT", p)))
        {
            native_leg(progress, &state.context, leg, &orders, true)?;
        }
        let matched = balanced_scope(
            &activity.order_ids,
            &activity.fill_ids,
            &orders,
            &fills,
            &state.context,
        )?;
        for (id, row) in matched {
            let at = clock(&row["fill"]["filledAt"])?;
            require(
                at >= state.context.created_at - chrono::Duration::seconds(1)
                    && at <= activity.flat_at
                    && result.fills.insert(id, row).is_none(),
                "capability fill clock or overlapping attribution invalid",
            )?;
        }
        let opening = authority["initial_broker_snapshot"].clone();
        require(
            opening["positions"] == json!([])
                && opening["orders"] == json!([])
                && opening["spendable_cash"]["currency"] == "EUR"
                && opening["account_id"] == activity.account_id
                && clock(&opening["observed_at"])? <= state.context.created_at,
            "capability opening snapshot invalid",
        )?;
        result.opening_accounts.push(opening);
    }
    for cleanup in &registry.external_cleanups {
        let proof = evidence["external_cleanup_attestation"]["scopes"]
            .as_array()
            .and_then(|rows| rows.iter().find(|r| r["scope"] == cleanup.scope_name))
            .ok_or_else(|| Error("external cleanup original attestation missing".into()))?;
        let proof_ids: Vec<String> = serde_json::from_value(proof["exact_ids"].clone())
            .map_err(|_| Error("external cleanup exact IDs missing".into()))?;
        require(
            ids(&proof_ids)? == ids(&cleanup.order_ids)?,
            "external cleanup exact ID set changed",
        )?;
        for (path, hash) in proof["original_artifacts"]
            .as_object()
            .ok_or_else(|| Error("external cleanup original hashes missing".into()))?
        {
            let raw = evidence["external_source_files"][path]
                .as_str()
                .ok_or_else(|| Error("external cleanup original file missing".into()))?;
            require(
                hash.as_str() == Some(trady_application::harness::content_hash(raw).as_str()),
                "external cleanup original file hash changed",
            )?;
        }

        let head = evidence["durable_heads"]
            .as_array()
            .and_then(|rows| {
                rows.iter().find(|r| {
                    r["owner_id"] == cleanup.owner_id
                        && r["account_id"] == cleanup.account_id
                        && r["intent_id"] == cleanup.intent_id
                })
            })
            .ok_or_else(|| Error("external cleanup original head missing".into()))?;
        let state: ProtocolState = serde_json::from_value(head["state"].clone())
            .map_err(|_| Error("external head typed state invalid".into()))?;
        state.context.validate()?;
        require(
            head["revision"] == cleanup.head_revision
                && head["state_sha256"] == cleanup.state_sha256
                && head["context"]["broker_account_id"] == 52211463
                && head["context"]["runtime_mode"] == "DEMO",
            "external cleanup original head changed",
        )?;
        require(
            state.revision == cleanup.head_revision
                && state_hash(&state)? == cleanup.state_sha256
                && serde_json::to_value(&state.context)
                    .map_err(|_| Error("context encoding failed".into()))?
                    == head["context"]
                && state.context.account_id == cleanup.account_id
                && state.context.intent_id == cleanup.intent_id,
            "external original state hash/context invalid",
        )?;
        let original_authority = proof["original_artifacts"]
            .as_object()
            .unwrap()
            .keys()
            .find(|p| p.ends_with("/authority.json"))
            .and_then(|p| evidence["external_source_files"][p].as_str())
            .ok_or_else(|| Error("external original authority missing".into()))?;
        let original: Value = serde_json::from_str(original_authority)
            .map_err(|_| Error("external original authority invalid".into()))?;
        require(
            original["context"] == head["context"]
                && original["classification"] == "BOUNDED_DEMO_CAPABILITY_EXPERIMENT_NOT_RELEASE",
            "external original authority context differs",
        )?;
        for (leg, progress) in state
            .entry
            .iter()
            .map(|p| ("ENTRY", p))
            .chain(state.protection.iter().map(|p| ("PROTECTION", p)))
            .chain(state.exits.iter().map(|p| ("EXIT", p)))
        {
            native_leg(progress, &state.context, leg, &orders, false)?;
        }
        let actual = balanced_scope(
            &cleanup.order_ids,
            &cleanup.fill_ids,
            &orders,
            &fills,
            &state.context,
        )?;
        let entry = head["state"]["entry"]["broker_id"]
            .as_str()
            .ok_or_else(|| Error("external cleanup exact entry missing".into()))?;
        require(
            cleanup.order_ids.iter().any(|v| v == entry)
                && actual.values().any(|r| {
                    r["order"]["id"].as_u64() == entry.parse::<u64>().ok()
                        && r["order"]["side"] == "BUY"
                }),
            "external cleanup does not cover original entry",
        )?;
        for id in head["state"]["protection"]
            .get("broker_id")
            .and_then(Value::as_str)
            .into_iter()
        {
            require(
                cleanup.order_ids.iter().any(|v| v == id),
                "external cleanup original STOP omitted",
            )?;
        }
    }
    // Every captured fill within this operation is either an exact strategy
    // order or a declared capability event. A reviewed empty registry cannot
    // silently erase known activity from account cash or performance.
    let mut strategy_orders = BTreeSet::new();
    let mut head_scopes = BTreeSet::new();
    for head in evidence["durable_heads"]
        .as_array()
        .ok_or_else(|| Error("durable heads missing".into()))?
    {
        let state = retained_head(head)?;
        let head_owner = head["owner_id"]
            .as_str()
            .ok_or_else(|| Error("durable owner missing".into()))?;
        require(
            head_scopes.insert((
                head_owner.to_owned(),
                state.context.account_id.clone(),
                state.context.intent_id.clone(),
            )),
            "duplicate durable head identity",
        )?;
        if head["owner_id"] == owner && head["account_id"] == account {
            strategy_orders.extend(
                state
                    .entry
                    .iter()
                    .chain(state.protection.iter())
                    .chain(state.exits.iter())
                    .filter_map(|p| p.broker_id.clone()),
            );
        }
    }
    for scope in evidence["observations"]["scopes"]
        .as_array()
        .ok_or_else(|| Error("capability observations missing".into()))?
    {
        let authority = &scope["authority"];
        if authority["classification"] != "BOUNDED_DEMO_CAPABILITY_EXPERIMENT_NOT_RELEASE" {
            continue;
        }
        let context: ProtocolContext = serde_json::from_value(authority["context"].clone())
            .map_err(|_| Error("capability scope context invalid".into()))?;
        if context.created_at >= registry.period_start && context.created_at < registry.period_end {
            require(
                registry.activities.iter().any(|a| {
                    scope["name"] == a.scope_name
                        && a.account_id == context.account_id
                        && a.intent_id == context.intent_id
                }),
                "retained current-period capability scope omitted",
            )?;
        }
    }
    let captured_non_strategy: BTreeSet<_> = fills
        .iter()
        .filter_map(|(id, row)| {
            let at = clock(&row["fill"]["filledAt"]).ok()?;
            (at >= registry.period_start
                && at < registry.period_end
                && !strategy_orders.contains(&row["order"]["id"].to_string()))
            .then_some(id.clone())
        })
        .collect();
    require(
        captured_non_strategy == result.fills.keys().cloned().collect(),
        "captured capability attribution is incomplete",
    )?;
    Ok((registry, result))
}

pub(super) fn classify(
    native: &BTreeMap<String, Value>,
    strategy: &BTreeMap<String, Value>,
    orders: &BTreeSet<String>,
    attribution: &CapabilityAttribution,
    start: DateTime<Utc>,
    cutoff: DateTime<Utc>,
) -> Result<(BTreeMap<String, Value>, BTreeSet<String>, bool)> {
    let mut expected = BTreeMap::new();
    for (id, row) in &attribution.fills {
        let at = clock(&row["fill"]["filledAt"])?;
        if at >= start && at < cutoff {
            expected.insert(id.clone(), row.clone());
        }
    }
    require(
        expected.keys().all(|id| !strategy.contains_key(id))
            && expected
                .values()
                .all(|r| !orders.contains(&r["order"]["id"].to_string())),
        "strategy and capability attribution overlap",
    )?;
    let mut strategy_native = BTreeMap::new();
    let mut unknown = BTreeSet::new();
    let mut complete = true;
    for (id, row) in native {
        if let Some(old) = expected.get(id) {
            require(old == row, "reviewed capability native fill changed")?;
        } else if orders.contains(&row["order"]["id"].to_string())
            && strategy.get(id).is_some_and(|old| {
                old["fill"] == row["fill"]
                    && ["id", "side", "quantity", "ticker", "instrument"]
                        .iter()
                        .all(|field| old["order"][field] == row["order"][field])
            })
        {
            strategy_native.insert(id.clone(), row.clone());
        } else {
            unknown.insert(row["order"]["id"].to_string());
        }
    }
    for id in expected.keys() {
        complete &= native.contains_key(id);
    }
    Ok((strategy_native, unknown, complete))
}

pub(super) fn command(args: &[String]) -> Result<Value> {
    let pending = args.iter().any(|v| v == "--review-candidate");
    let args: Vec<_> = args
        .iter()
        .filter(|v| v.as_str() != "--review-candidate")
        .cloned()
        .collect();
    let opts = super::eur::options(
        &args,
        &["--registry", "--evidence"],
        &["--registry", "--evidence"],
    )?;
    let header: Registry = read(Path::new(&opts["--registry"]))?;
    let (registry, facts) = load(
        Path::new(&opts["--registry"]),
        Path::new(&opts["--evidence"]),
        &header.owner_id,
        &header.account_id,
        Utc::now(),
        pending,
    )?;
    Ok(
        json!({"status":if pending{"CAPABILITY_PROOF_VALIDATED_PENDING_REVIEW"}else{"REVIEWED_CAPABILITY_ATTRIBUTION_VALID"},"capability_fill_count":facts.fills.len(),"external_cleanup_count":registry.external_cleanups.len(),"release_accepted":false,"broker_mutations":0,"provider_calls":0,"live_enabled":false}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn row(order: u64, fill: u64, side: &str) -> Value {
        json!({"order":{"id":order,"side":side,"quantity":1},"fill":{"id":fill,"filledAt":"2026-11-06T08:00:00Z","walletImpact":{"netValue":100}}})
    }
    #[test]
    fn exact_capability_partition_cannot_hide_overlap_missing_changed_or_net_zero_extra_pairs() {
        let at: DateTime<Utc> = "2026-11-06T00:00:00Z".parse().unwrap();
        let end = at + chrono::Duration::days(1);
        let capability = BTreeMap::from([
            ("1".into(), row(11, 1, "BUY")),
            ("2".into(), row(12, 2, "SELL")),
        ]);
        let attribution = CapabilityAttribution {
            fills: capability.clone(),
            opening_accounts: vec![],
        };
        let strategy = BTreeMap::from([
            ("3".into(), row(13, 3, "BUY")),
            ("4".into(), row(14, 4, "SELL")),
        ]);
        let orders = BTreeSet::from(["13".into(), "14".into()]);
        let mut native = capability.clone();
        native.extend(strategy.clone());
        let (partition, unknown, complete) =
            classify(&native, &strategy, &orders, &attribution, at, end).unwrap();
        assert_eq!(partition, strategy);
        assert!(unknown.is_empty() && complete);
        native.remove("1");
        assert!(
            !classify(&native, &strategy, &orders, &attribution, at, end)
                .unwrap()
                .2
        );
        native = capability.clone();
        native.extend(strategy.clone());
        native.get_mut("1").unwrap()["fill"]["walletImpact"]["netValue"] = json!(101);
        assert!(classify(&native, &strategy, &orders, &attribution, at, end).is_err());
        native = capability;
        native.extend(strategy.clone());
        let mut overlap = strategy.clone();
        overlap.insert("1".into(), native["1"].clone());
        assert!(classify(&native, &overlap, &orders, &attribution, at, end).is_err());
        let mut wrong_orders = orders.clone();
        wrong_orders.insert("11".into());
        assert!(classify(&native, &strategy, &wrong_orders, &attribution, at, end).is_err());
        native.insert("5".into(), row(15, 5, "BUY"));
        native.insert("6".into(), row(16, 6, "SELL"));
        assert_eq!(
            classify(&native, &strategy, &orders, &attribution, at, end)
                .unwrap()
                .0,
            strategy,
            "unknown fills are never labeled strategy performance"
        );
        assert_eq!(
            classify(&native, &strategy, &orders, &attribution, at, end)
                .unwrap()
                .1,
            BTreeSet::from(["15".into(), "16".into()])
        );
    }

    #[test]
    fn durable_head_metadata_cannot_reclassify_capability_as_strategy() {
        let at: DateTime<Utc> = "2026-11-06T08:00:00Z".parse().unwrap();
        let context:ProtocolContext=serde_json::from_value(json!({"runtime_mode":"DEMO","policy_revision":"eur_intraday_v1","intent_id":"cap_fixture","account_id":"paper_cap_fixture","broker_account_id":52211463,"instrument_id":"AIR_PA_EQ","broker_ticker":"AIRp_EQ","isin":"NL0000235190","policy_hash":"a".repeat(64),"configuration_hash":"b".repeat(64),"evidence_ids":["synthetic_fixture_only"],"quantity":"1","entry_limit":"100","initial_stop":"99","target":"102","created_at":at,"flatten_at":at+chrono::Duration::minutes(10),"max_unprotected_seconds":10})).unwrap();
        let state = ProtocolState::new(context.clone()).unwrap();
        let head = json!({"owner_id":"synthetic_owner","account_id":context.account_id,"intent_id":context.intent_id,"revision":state.revision,"state_sha256":state_hash(&state).unwrap(),"context":context,"state":state});
        retained_head(&head).unwrap();
        let mut changed = head.clone();
        changed["account_id"] = json!("paper_t212_eur_demo");
        assert!(retained_head(&changed).is_err());
        let mut changed = head;
        changed["intent_id"] = json!("strategy_intent");
        assert!(retained_head(&changed).is_err());
    }

    #[test]
    fn retained_source_bytes_and_projections_must_agree() {
        let mut proof = json!({"source_files":{},"source_bytes":{},"source_documents":{}});
        for name in [
            "observations",
            "native_history",
            "external_cleanup_attestation",
            "current_flat_observation",
            "durable_heads",
        ] {
            let path = format!("synthetic/{name}.json");
            proof["source_files"][&path] = json!(trady_application::harness::content_hash("{}\n"));
            proof["source_bytes"][&path] = json!("{}\n");
            proof["source_documents"][name] = json!(path);
            proof[name] = json!({});
        }
        source_projections(&proof).unwrap();
        let mut changed = proof.clone();
        changed["observations"] = json!({"manufactured":true});
        assert!(source_projections(&changed).is_err());
        let mut changed = proof.clone();
        changed["source_files"]["synthetic/observations.json"] = json!("0".repeat(64));
        assert!(source_projections(&changed).is_err());
        proof["source_bytes"]["synthetic/observations.json"] = json!("{\"changed\":true}\n");
        assert!(source_projections(&proof).is_err());
    }

    #[test]
    fn native_history_requires_a_complete_cursor_chain() {
        let page = json!({"provider":"trading212_demo","http_status":200,"endpoint":"https://demo.trading212.com/api/v0/equity/history/orders","parameters":{"limit":50},"requested_at":"2026-11-06T08:00:00Z","received_at":"2026-11-06T08:00:01Z","broker_mutations":0,"response":{"items":[],"nextPagePath":null}});
        let proof = json!({"native_history":{"classification":"BOUNDED_READ_ONLY_ACCOUNT_WIDE_NATIVE_HISTORY","broker_mutations":0,"entry_authorized":false,"complete":true,"page_count":1,"pages":[page.clone()]}});
        native_rows(&proof).unwrap();
        let mut truncated = proof.clone();
        truncated["native_history"]["pages"][0]["response"]["nextPagePath"] =
            json!("/api/v0/equity/history/orders?cursor=123&limit=50");
        assert!(native_rows(&truncated).is_err());
        let mut chained = truncated.clone();
        chained["native_history"]["page_count"] = json!(2);
        chained["native_history"]["pages"]
            .as_array_mut()
            .unwrap()
            .push(page);
        assert!(native_rows(&chained).is_err());
        chained["native_history"]["pages"][1]["parameters"]["cursor"] = json!("123");
        native_rows(&chained).unwrap();
        chained["native_history"]["pages"][0]["parameters"]["ticker"] = json!("AIRp_EQ");
        assert!(
            native_rows(&chained).is_err(),
            "filtered history cannot prove account-wide coverage"
        );
    }
}

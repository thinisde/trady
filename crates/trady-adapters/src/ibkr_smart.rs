//! Read-only IBKR SMART quotes through the bounded official Python SDK bridge.
//! SMART is a composite source; these observations confer no Trading 212 entry authority.
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use trady_application::{eur_shadow::ShadowIdentity, harness::content_hash};
use trady_domain::{Error, InstrumentId, Result, require};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IbkrSmartConfig {
    pub config_version: u32,
    pub enabled: bool,
    pub base_currency: String,
    pub authority: String,
    pub python_path: String,
    pub script_path: String,
    pub script_sha256: String,
    pub sdk_version: String,
    pub sdk_archive_sha256: String,
    pub port: u16,
    pub client_id: i32,
    pub capture_seconds: u8,
    pub timeout_seconds: u8,
    pub bindings: BTreeMap<InstrumentId, IbkrBinding>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IbkrBinding {
    pub con_id: i64,
    pub isin: String,
    pub broker_ticker: String,
    pub primary_exchange: String,
    pub exchange_mic: String,
}
impl IbkrSmartConfig {
    pub fn validate(&self) -> Result<()> {
        require(
            self.config_version == 1
                && self.base_currency == "EUR"
                && self.authority == "SHADOW_QUOTES_ONLY"
                && self.python_path.starts_with("/opt/trady/")
                && self.script_path.starts_with("/opt/trady/")
                && [&self.python_path, &self.script_path].iter().all(|p| {
                    !Path::new(p)
                        .components()
                        .any(|c| matches!(c, std::path::Component::ParentDir))
                })
                && [&self.script_sha256, &self.sdk_archive_sha256]
                    .iter()
                    .all(|s| {
                        s.len() == 64
                            && s.bytes()
                                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
                    })
                && self.sdk_version == "10.50.2"
                && self.port > 0
                && self.client_id > 0
                && (5..=10).contains(&self.capture_seconds)
                && (40..=90).contains(&self.timeout_seconds)
                && !self.bindings.is_empty()
                && self.bindings.len() <= 8,
            "invalid read-only IBKR quote configuration",
        )?;
        let mut con_ids = std::collections::BTreeSet::new();
        for (id, b) in &self.bindings {
            require(
                id.as_str().ends_with("_EQ")
                    && b.con_id > 0
                    && con_ids.insert(b.con_id)
                    && b.isin.len() == 12
                    && b.isin.bytes().all(|c| c.is_ascii_alphanumeric())
                    && b.broker_ticker.ends_with("_EQ")
                    && matches!(
                        (b.exchange_mic.as_str(), b.primary_exchange.as_str()),
                        ("XPAR", "SBF") | ("XAMS", "AEB")
                    ),
                "unreviewed or ambiguous IBKR EUR contract",
            )?;
        }
        Ok(())
    }
    pub fn validate_identity(&self, identity: &ShadowIdentity) -> Result<&IbkrBinding> {
        self.validate()?;
        let binding = self
            .bindings
            .get(&identity.internal_instrument_id)
            .ok_or_else(|| Error("IBKR identity binding missing".into()))?;
        require(
            binding.isin == identity.isin
                && binding.broker_ticker == identity.broker_ticker
                && binding.exchange_mic == identity.exchange_mic,
            "IBKR binding differs from broker crosswalk",
        )?;
        Ok(binding)
    }
    pub fn capture(&self, crosswalk: &Path, out: &Path) -> Result<Value> {
        self.validate()?;
        require(self.enabled, "IBKR quote source disabled")?;
        let script = fs::read_to_string(&self.script_path)
            .map_err(|_| Error("IBKR read-only bridge unavailable".into()))?;
        require(
            content_hash(&script) == self.script_sha256,
            "IBKR bridge hash changed",
        )?;
        for path in [&self.script_path, &self.python_path] {
            let metadata =
                fs::metadata(path).map_err(|_| Error("IBKR SDK path unavailable".into()))?;
            require(metadata.is_file(), "IBKR SDK path is not a file")?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                require(
                    metadata.permissions().mode() & 0o022 == 0,
                    "IBKR SDK path is writable by another role",
                )?;
            }
        }
        require(
            out.starts_with("/var/lib/")
                && !out
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir)),
            "IBKR capture requires persistent state directory",
        )?;
        let mut child = Command::new(&self.python_path)
            .env_clear()
            .env("PYTHONNOUSERSITE", "1")
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .arg(&self.script_path)
            .args(["--crosswalk"])
            .arg(crosswalk)
            .arg("--out")
            .arg(out)
            .args([
                "--port",
                &self.port.to_string(),
                "--client-id",
                &self.client_id.to_string(),
                "--seconds",
                &self.capture_seconds.to_string(),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| Error("IBKR read-only bridge failed to start".into()))?;
        let start = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None)
                    if start.elapsed() < Duration::from_secs(u64::from(self.timeout_seconds)) =>
                {
                    thread::sleep(Duration::from_millis(50))
                }
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(Error("IBKR quote capture timed out or disconnected".into()));
                }
            }
        }
        let metadata =
            fs::metadata(out).map_err(|_| Error("IBKR quote capture unavailable".into()))?;
        require(
            metadata.len() <= 4_000_000,
            "IBKR capture exceeds durable size bound",
        )?;
        let mut value: Value = serde_json::from_slice(
            &fs::read(out).map_err(|_| Error("IBKR capture unreadable".into()))?,
        )
        .map_err(|_| Error("IBKR capture invalid".into()))?;
        if value["status"] != "COMPLETED" {
            value["provider_status"] = json!("UNAVAILABLE");
            value["error_code"] = json!("IBKR_CAPTURE_INCOMPLETE");
        }
        Ok(value)
    }
    /// Use provider-timestamped BidAsk callbacks, never the local receipt time of
    /// separate top-of-book fields as a substitute for a synchronized book time.
    pub fn normalize(
        &self,
        identity: &ShadowIdentity,
        raw: &Value,
        now: DateTime<Utc>,
        max_age: i64,
    ) -> Result<Value> {
        let binding = self.validate_identity(identity)?;
        require(
            (1..=60).contains(&max_age)
                && raw["status"] == "COMPLETED"
                && raw["requested_market_data_type"] == 1
                && raw["events_truncated"] == false
                && raw["sdk_version"] == self.sdk_version
                && raw["probe_source_sha256"] == self.script_sha256
                && raw["broker_mutations"] == 0
                && raw["regulatory_snapshots"] == false
                && raw["read_only_outgoing_message_allowlist"] == true,
            "IBKR capture unavailable, delayed or untrusted",
        )?;
        let instruments = raw["instruments"]
            .as_array()
            .ok_or_else(|| Error("IBKR contract evidence missing".into()))?;
        let matching: Vec<_> = instruments
            .iter()
            .filter(|v| {
                v["instrument_id"].as_str() == Some(identity.internal_instrument_id.as_str())
            })
            .collect();
        require(matching.len() == 1, "ambiguous IBKR contract observation")?;
        let instrument = matching[0];
        let contracts = instrument["contracts"]
            .as_array()
            .ok_or_else(|| Error("IBKR contract details missing".into()))?;
        require(
            instrument["unique_exact_isin_eur_contract"] == true
                && instrument["contract_resolution_complete"] == true
                && instrument["isin"] == binding.isin
                && instrument["broker_ticker"] == binding.broker_ticker
                && contracts.len() == 1
                && contracts[0]["con_id"].as_i64() == Some(binding.con_id)
                && contracts[0]["identifiers"]["ISIN"] == binding.isin
                && contracts[0]["currency"] == "EUR"
                && contracts[0]["security_type"] == "STK"
                && contracts[0]["exchange"] == "SMART"
                && contracts[0]["primary_exchange"] == binding.primary_exchange,
            "IBKR conId, ISIN, currency or primary listing changed",
        )?;
        let events = raw["events"]
            .as_array()
            .ok_or_else(|| Error("IBKR quote events missing".into()))?;
        let quote_id = instrument["quote_request_id"]
            .as_i64()
            .ok_or_else(|| Error("IBKR quote request ID missing".into()))?;
        let book_id = instrument["book_request_id"]
            .as_i64()
            .ok_or_else(|| Error("IBKR book request ID missing".into()))?;
        let data_type = events.iter().rev().find(|e| {
            e["kind"] == "market_data_type" && e["request_id"].as_i64() == Some(quote_id)
        });
        require(
            data_type.is_some_and(|e| e["data_type"] == 1),
            "IBKR live data type not confirmed",
        )?;
        let book = events
            .iter()
            .rev()
            .find(|e| e["kind"] == "bid_ask" && e["request_id"].as_i64() == Some(book_id))
            .ok_or_else(|| Error("IBKR timestamped bid/ask unavailable".into()))?;
        let observed = book["observed_at_seconds"]
            .as_i64()
            .and_then(|s| DateTime::from_timestamp(s, 0))
            .ok_or_else(|| Error("IBKR book time missing".into()))?;
        let received: DateTime<Utc> = serde_json::from_value(book["received_at"].clone())
            .map_err(|_| Error("IBKR receipt time missing".into()))?;
        let decimal = |key: &str| -> Result<Decimal> {
            book[key]
                .as_str()
                .and_then(|s| s.parse::<Decimal>().ok())
                .ok_or_else(|| Error("IBKR book price or size missing".into()))
        };
        let (bid, ask, bid_size, ask_size) = (
            decimal("bid")?,
            decimal("ask")?,
            decimal("bid_size")?,
            decimal("ask_size")?,
        );
        require(
            bid > Decimal::ZERO
                && ask >= bid
                && bid_size > Decimal::ZERO
                && ask_size > Decimal::ZERO
                && observed <= received
                && received <= now,
            "IBKR book crossed, empty or future-dated",
        )?;
        let fresh = now.signed_duration_since(observed).num_milliseconds() <= max_age * 1000;
        Ok(
            json!({"instrument_id":identity.internal_instrument_id,"symbol":format!("IBKR:{}",binding.con_id),
            "provider_instrument_id":binding.con_id,"isin":binding.isin,"broker_ticker":binding.broker_ticker,
            "currency":"EUR","source_venue":"SMART","primary_exchange":binding.primary_exchange,
            "bid":bid,"ask":ask,"bid_size":bid_size,"ask_size":ask_size,"last":null,
            "observed_at":observed,"received_at":received,"market_state":"UNKNOWN","fresh":fresh,
            "normalized_at":now,
            "market_data_type":1,"quote_quality":if fresh{"LIVE_TWO_SIDED_COMPOSITE"}else{"STALE"},
            "quote_to_broker_compatibility":"UNACCEPTED","entry_authorized":false}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration as Delta;
    fn fixture() -> (IbkrSmartConfig, ShadowIdentity, Value, DateTime<Utc>) {
        let config: IbkrSmartConfig = serde_json::from_str(include_str!(
            "../../../config/ibkr-smart.eur-shadow.example.json"
        ))
        .unwrap();
        let crosswalk: trady_application::eur_shadow::ShadowCrosswalk = serde_json::from_str(
            include_str!("../../../docs/evidence/eur-shadow-review-2026-11-04/crosswalk.json"),
        )
        .unwrap();
        let identity = crosswalk
            .members
            .iter()
            .find(|v| v.broker_ticker == "AIRp_EQ")
            .unwrap()
            .clone();
        let now: DateTime<Utc> = "2026-11-04T13:00:01Z".parse().unwrap();
        let raw = json!({"status":"COMPLETED","requested_market_data_type":1,"events_truncated":false,
            "sdk_version":config.sdk_version,"probe_source_sha256":config.script_sha256,
            "broker_mutations":0,"regulatory_snapshots":false,"read_only_outgoing_message_allowlist":true,
            "instruments":[{"instrument_id":"AIR_PA_EQ","isin":identity.isin,"broker_ticker":identity.broker_ticker,
                "unique_exact_isin_eur_contract":true,"contract_resolution_complete":true,
                "quote_request_id":1000,"book_request_id":2000,
                "contracts":[{"con_id":29612256,"identifiers":{"ISIN":identity.isin},"currency":"EUR",
                    "security_type":"STK","exchange":"SMART","primary_exchange":"SBF"}]}],
            "events":[{"kind":"market_data_type","request_id":1000,"data_type":1},
                {"kind":"bid_ask","request_id":2000,"observed_at_seconds":now.timestamp()-1,
                    "received_at":now,"bid":"100","ask":"100.01","bid_size":"25","ask_size":"50"}]});
        (config, identity, raw, now)
    }
    #[test]
    fn live_synchronized_book_keeps_its_exact_identity_and_provider_time() {
        let (c, i, raw, now) = fixture();
        let quote = c.normalize(&i, &raw, now, 10).unwrap();
        assert_eq!(quote["symbol"], "IBKR:29612256");
        assert_eq!(quote["fresh"], true);
        assert_eq!(quote["market_state"], "UNKNOWN");
        assert_eq!(quote["entry_authorized"], false);
        let stale = c.normalize(&i, &raw, now + Delta::seconds(10), 10).unwrap();
        assert_eq!(stale["fresh"], false);
        assert_eq!(stale["observed_at"], quote["observed_at"]);
        let mut regular = quote;
        regular["market_state"] = json!("REGULAR");
        let obs = trady_application::eur_shadow::ProviderObservation {
            observation_id: "ibkr_fixture".into(),
            provider: "ibkr_tws_api".into(),
            endpoint: "tcp://127.0.0.1/ib-gateway".into(),
            source_id: "fixture".into(),
            requested_at: now,
            received_at: now,
            published_at: None,
            response_hash: content_hash(&regular.to_string()),
            rights_reference: "synthetic_fixture".into(),
            status: trady_application::eur_shadow::ObservationStatus::Ok,
            error_code: None,
            capture_level: "SYNTHETIC_FIXTURE".into(),
            response: regular,
        };
        let candidate = trady_application::eur_shadow::assess_quote(&i, &obs, now, 10).unwrap();
        assert!(candidate.research_eligible);
        assert!(!candidate.entry_eligible);
    }
    #[test]
    fn delayed_frozen_rebound_crossed_and_untimed_quotes_are_rejected() {
        let (c, i, raw, now) = fixture();
        for (pointer, value) in [
            ("/events/0/data_type", json!(2)),
            ("/events/0/data_type", json!(3)),
            ("/events/0/data_type", json!(4)),
            ("/events/1/kind", json!("price")),
            ("/events/1/observed_at_seconds", Value::Null),
            ("/events/1/observed_at_seconds", json!(now.timestamp() + 1)),
            ("/events/1/ask", json!("99")),
            ("/events/1/bid_size", json!("0")),
            ("/instruments/0/contracts/0/con_id", json!(1)),
            ("/instruments/0/contracts/0/currency", json!("USD")),
            ("/instruments/0/contracts/0/primary_exchange", json!("AEB")),
        ] {
            let mut bad = raw.clone();
            *bad.pointer_mut(pointer).unwrap() = value;
            assert!(c.normalize(&i, &bad, now, 10).is_err(), "{pointer}");
        }
        let mut unknown = raw;
        unknown["events"] = json!([]);
        assert!(c.normalize(&i, &unknown, now, 10).is_err());
    }
}

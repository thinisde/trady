//! Completed trade candles from the existing read-only SMART connection.
//! Candles never replace the separately timestamped executable bid/ask book.
use crate::ibkr_smart::IbkrSmartConfig;
use chrono::{DateTime, Duration as ChronoDuration, FixedOffset, NaiveDateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use trady_application::{eur_shadow::ShadowIdentity, harness::content_hash};
use trady_domain::{BarSnapshot, CompletedBar, Error, Result, require};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentBarsConfig {
    pub config_version: u32,
    pub enabled: bool,
    pub provider: String,
    pub python_path: String,
    pub script_path: String,
    pub script_sha256: String,
    pub client_id: i32,
    pub timeout_seconds: u8,
    pub interval_seconds: u32,
    pub duration_days: u8,
}
fn hash(value: &Value) -> Result<String> {
    Ok(content_hash(&serde_json::to_string(value).map_err(
        |_| Error("current-bar encoding failed".into()),
    )?))
}
fn coverage_time(coverage: &Value, key: &str) -> Result<DateTime<Utc>> {
    let raw = coverage[key]
        .as_str()
        .ok_or_else(|| Error("provider coverage time missing".into()))?;
    let (local, zone) = raw
        .rsplit_once(' ')
        .ok_or_else(|| Error("provider coverage zone missing".into()))?;
    let offset = coverage[format!("{key}_offset_seconds")]
        .as_i64()
        .ok_or_else(|| Error("provider coverage offset missing".into()))?;
    require(
        match zone {
            "UTC" | "GMT" => offset == 0,
            "MET" | "Europe/Berlin" | "Europe/Paris" | "Europe/Amsterdam" => {
                matches!(offset, 3600 | 7200)
            }
            _ => false,
        },
        "unsupported provider coverage zone/offset",
    )?;
    let local = NaiveDateTime::parse_from_str(local, "%Y%m%d %H:%M:%S")
        .map_err(|_| Error("invalid native provider coverage time".into()))?;
    let parsed = FixedOffset::east_opt(offset as i32)
        .and_then(|offset| offset.from_local_datetime(&local).single())
        .ok_or_else(|| Error("invalid native provider coverage offset".into()))?
        .with_timezone(&Utc);
    let retained: DateTime<Utc> = serde_json::from_value(coverage[format!("{key}_utc")].clone())
        .map_err(|_| Error("provider UTC coverage missing".into()))?;
    require(
        parsed == retained,
        "provider native and UTC coverage differ",
    )?;
    Ok(parsed)
}
impl CurrentBarsConfig {
    pub fn validate(&self) -> Result<()> {
        require(
            self.config_version == 1
                && self.provider == "IBKR_SMART_TRADES"
                && self.interval_seconds == 300
                && self.duration_days == 2
                && self.client_id > 0
                && (40..=90).contains(&self.timeout_seconds)
                && self.script_sha256.len() == 64
                && self
                    .script_sha256
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
                && [&self.python_path, &self.script_path].iter().all(|p| {
                    p.starts_with("/opt/trady/")
                        && !Path::new(p)
                            .components()
                            .any(|c| matches!(c, std::path::Component::ParentDir))
                }),
            "invalid readonly current-bar configuration",
        )
    }
    pub fn capture(&self, quote_config: &Path, crosswalk: &Path, out: &Path) -> Result<Value> {
        self.validate()?;
        require(self.enabled, "current-bar source disabled")?;
        for path in [&self.python_path, &self.script_path] {
            let metadata =
                fs::metadata(path).map_err(|_| Error("current-bar bridge unavailable".into()))?;
            require(metadata.is_file(), "current-bar bridge path is not a file")?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                require(
                    metadata.permissions().mode() & 0o022 == 0,
                    "current-bar bridge is writable by another role",
                )?;
            }
        }
        require(
            content_hash(
                &fs::read_to_string(&self.script_path)
                    .map_err(|_| Error("current-bar bridge unreadable".into()))?,
            ) == self.script_sha256,
            "current-bar bridge hash changed",
        )?;
        require(
            out.starts_with("/var/lib/")
                && !out
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir)),
            "current bars require persistent state",
        )?;
        let mut child = Command::new(&self.python_path)
            .env_clear()
            .env("PYTHONNOUSERSITE", "1")
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .arg(&self.script_path)
            .arg("--quote-config")
            .arg(quote_config)
            .arg("--crosswalk")
            .arg(crosswalk)
            .arg("--out")
            .arg(out)
            .arg("--client-id")
            .arg(self.client_id.to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| Error("current-bar bridge did not start".into()))?;
        let started = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None)
                    if started.elapsed() < Duration::from_secs(u64::from(self.timeout_seconds)) =>
                {
                    thread::sleep(Duration::from_millis(50))
                }
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(Error("current-bar capture timed out".into()));
                }
            }
        }
        let metadata = fs::symlink_metadata(out)
            .map_err(|_| Error("current-bar capture unavailable".into()))?;
        require(
            metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() <= 2_000_000,
            "invalid current-bar capture file",
        )?;
        let mut value: Value = serde_json::from_slice(
            &fs::read(out).map_err(|_| Error("current-bar capture unreadable".into()))?,
        )
        .map_err(|_| Error("invalid current-bar capture".into()))?;
        if value["status"] != "COMPLETED" {
            value["provider_status"] = Value::String("UNAVAILABLE".into());
            value["error_code"] = Value::String("IBKR_CURRENT_BARS_INCOMPLETE".into());
        }
        Ok(value)
    }
    pub fn normalize(
        &self,
        quotes: &IbkrSmartConfig,
        identity: &ShadowIdentity,
        crosswalk_hash: &str,
        raw: &Value,
        now: DateTime<Utc>,
    ) -> Result<BarSnapshot> {
        self.validate()?;
        let binding = quotes.validate_identity(identity)?;
        require(
            self.enabled
                && quotes.enabled
                && raw["status"] == "COMPLETED"
                && raw["provider"] == "IBKR_SMART_TRADES"
                && raw["probe_source_sha256"] == self.script_sha256
                && raw["sdk_version"] == quotes.sdk_version
                && raw["quote_config_canonical_sha256"]
                    == hash(
                        &serde_json::to_value(quotes)
                            .map_err(|_| Error("quote settings invalid".into()))?,
                    )?
                && raw["crosswalk_canonical_sha256"] == crosswalk_hash
                && raw["client_id"] == self.client_id
                && raw["interval_seconds"] == 300
                && raw["duration_days"] == 2
                && raw["what_to_show"] == "TRADES"
                && raw["rth_only"] == true
                && raw["read_only_outgoing_message_allowlist"] == true
                && raw["broker_mutations"] == 0
                && raw["regulatory_snapshots"] == false,
            "current-bar source identity or scope differs",
        )?;
        let data = &raw["instruments"][identity.internal_instrument_id.as_str()];
        let contract = &data["contract"];
        require(
            data["historical_end_received"] == true
                && contract["con_id"] == binding.con_id
                && contract["currency"] == "EUR"
                && contract["security_type"] == "STK"
                && contract["exchange"] == "SMART"
                && contract["primary_exchange"] == binding.primary_exchange
                && contract["identifiers"]["ISIN"] == identity.isin,
            "current-bar contract/history coverage differs",
        )?;
        let requested_at: DateTime<Utc> = serde_json::from_value(data["requested_at"].clone())
            .map_err(|_| Error("bar request time missing".into()))?;
        let received_at: DateTime<Utc> = serde_json::from_value(data["received_at"].clone())
            .map_err(|_| Error("bar receipt time missing".into()))?;
        require(
            requested_at <= received_at
                && received_at <= now
                && (received_at - requested_at).num_seconds() <= i64::from(self.timeout_seconds),
            "bar capture clock invalid",
        )?;
        let coverage_start = coverage_time(&data["coverage"], "start")?;
        let coverage_end = coverage_time(&data["coverage"], "end")?;
        require(
            coverage_start < coverage_end && coverage_end <= requested_at,
            "provider coverage bounds invalid",
        )?;
        let rows = data["raw_bars"]
            .as_array()
            .ok_or_else(|| Error("raw bars missing".into()))?;
        require(
            !rows.is_empty() && rows.len() <= 512,
            "raw bar count invalid",
        )?;
        let last_open = rows
            .last()
            .and_then(|r| r["date"].as_str())
            .and_then(|s| s.parse::<i64>().ok())
            .and_then(|at| DateTime::from_timestamp(at, 0))
            .ok_or_else(|| Error("last native candle epoch invalid".into()))?;
        // A succeeding native candle proves the previous interval finished.
        // The provider's request-end marker alone cannot establish its data lag.
        let cutoff = requested_at.min(coverage_end).min(last_open);
        let mut bars = Vec::new();
        let mut prior = None;
        for row in rows {
            let epoch: i64 = row["date"]
                .as_str()
                .ok_or_else(|| Error("provider bar epoch missing".into()))?
                .parse()
                .map_err(|_| Error("provider bar epoch invalid".into()))?;
            let opens_at = DateTime::from_timestamp(epoch, 0)
                .ok_or_else(|| Error("provider bar epoch invalid".into()))?;
            require(
                opens_at >= coverage_start
                    && epoch.rem_euclid(300) == 0
                    && opens_at <= requested_at
                    && prior.is_none_or(|old| epoch > old),
                "provider bar grid/order invalid",
            )?;
            prior = Some(epoch);
            let closes_at = opens_at + ChronoDuration::seconds(300);
            let decimal = |key: &str| {
                serde_json::from_value(row[key].clone())
                    .map_err(|_| Error("provider OHLCV missing or invalid".into()))
            };
            let bar = CompletedBar {
                bar_id: format!("ibkr_{}_{}", identity.internal_instrument_id, epoch),
                instrument_id: identity.internal_instrument_id.clone(),
                interval_seconds: 300,
                opens_at,
                closes_at,
                open: decimal("open")?,
                high: decimal("high")?,
                low: decimal("low")?,
                close: decimal("close")?,
                volume: Some(decimal("volume")?),
                provider: "ibkr_smart_trades".into(),
                received_at,
                completed: true,
            };
            // A valid still-forming native candle is retained raw, never a signal candle.
            if closes_at > cutoff {
                continue;
            }
            bars.push(bar);
        }
        let observed_at = bars
            .last()
            .ok_or_else(|| Error("no completed current bars".into()))?
            .closes_at;
        let result = BarSnapshot {
            snapshot_id: format!(
                "ibkr_bars_{}_{}",
                identity.internal_instrument_id,
                requested_at.timestamp_millis()
            ),
            instrument_id: identity.internal_instrument_id.clone(),
            provider: "ibkr_smart_trades".into(),
            interval_seconds: 300,
            observed_at,
            received_at,
            bars,
        };
        result.validate(now)?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn fixture() -> (
        CurrentBarsConfig,
        IbkrSmartConfig,
        ShadowIdentity,
        Value,
        DateTime<Utc>,
    ) {
        let settings = CurrentBarsConfig {
            config_version: 1,
            enabled: true,
            provider: "IBKR_SMART_TRADES".into(),
            python_path: "/opt/trady/ibkr-sdk/bin/python".into(),
            script_path: "/opt/trady/scripts/capture-ibkr-current-bars.py".into(),
            script_sha256: "a".repeat(64),
            client_id: 910075,
            timeout_seconds: 90,
            interval_seconds: 300,
            duration_days: 2,
        };
        let mut quotes: IbkrSmartConfig = serde_json::from_str(include_str!(
            "../../../config/ibkr-smart.eur-shadow.example.json"
        ))
        .unwrap();
        quotes.enabled = true;
        let crosswalk: trady_application::eur_shadow::ShadowCrosswalk = serde_json::from_str(
            include_str!("../../../docs/evidence/eur-shadow-review-2026-11-04/crosswalk.json"),
        )
        .unwrap();
        let identity = crosswalk
            .members
            .into_iter()
            .find(|m| m.internal_instrument_id.as_str() == "AIR_PA_EQ")
            .unwrap();
        let now: DateTime<Utc> = "2026-11-05T15:45:13Z".parse().unwrap();
        let rows:Vec<_>=(0..3).map(|i|json!({"date":(now.timestamp()-13-600+i*300).to_string(),"open":"200","high":"201","low":"199","close":"200","volume":"100"})).collect();
        let mut raw = json!({"status":"COMPLETED","provider":"IBKR_SMART_TRADES","probe_source_sha256":settings.script_sha256,"sdk_version":quotes.sdk_version,
            "quote_config_canonical_sha256":hash(&serde_json::to_value(&quotes).unwrap()).unwrap(),"crosswalk_canonical_sha256":"b".repeat(64),"client_id":910075,"interval_seconds":300,"duration_days":2,"what_to_show":"TRADES","rth_only":true,"read_only_outgoing_message_allowlist":true,"broker_mutations":0,"regulatory_snapshots":false,
            "instruments":{"AIR_PA_EQ":{"contract":{"con_id":29612256,"currency":"EUR","security_type":"STK","exchange":"SMART","primary_exchange":"SBF","identifiers":{"ISIN":"NL0000235190"}},"historical_end_received":true,"requested_at":now-ChronoDuration::seconds(1),"received_at":now,"raw_bars":rows}}});
        let start = now - ChronoDuration::days(2);
        let end = now - ChronoDuration::seconds(1);
        raw["instruments"]["AIR_PA_EQ"]["coverage"] = json!({"start":start.format("%Y%m%d %H:%M:%S UTC").to_string(),"start_utc":start,"start_offset_seconds":0,"end":end.format("%Y%m%d %H:%M:%S UTC").to_string(),"end_utc":end,"end_offset_seconds":0});
        (settings, quotes, identity, raw, now)
    }
    #[test]
    fn provider_times_exclude_forming_bar_without_inventing_freshness_or_quotes() {
        let (settings, quotes, identity, raw, now) = fixture();
        let bars = settings
            .normalize(&quotes, &identity, &"b".repeat(64), &raw, now)
            .unwrap();
        assert_eq!(bars.bars.len(), 2);
        assert_eq!(bars.observed_at, now - ChronoDuration::seconds(13));
        assert_eq!(bars.provider, "ibkr_smart_trades");
        assert_eq!(bars.bars.last().unwrap().closes_at, bars.observed_at);
        let stale_at = now + ChronoDuration::minutes(11);
        let stale = settings
            .normalize(&quotes, &identity, &"b".repeat(64), &raw, stale_at)
            .unwrap();
        assert_eq!(stale.observed_at, bars.observed_at);
        assert!(!trady_application::eur_shadow::completed_bar_is_current(
            &stale,
            now - ChronoDuration::hours(4),
            stale_at
        ));
    }
    #[test]
    fn native_provider_cutoff_excludes_a_candle_ending_after_coverage() {
        let (settings, quotes, identity, mut raw, now) = fixture();
        let end = now - ChronoDuration::minutes(2);
        raw["instruments"]["AIR_PA_EQ"]["coverage"]["end"] =
            json!(end.format("%Y%m%d %H:%M:%S UTC").to_string());
        raw["instruments"]["AIR_PA_EQ"]["coverage"]["end_utc"] = json!(end);
        let result = settings
            .normalize(&quotes, &identity, &"b".repeat(64), &raw, now)
            .unwrap();
        assert_eq!(result.bars.len(), 1);
        assert_eq!(
            result.observed_at,
            "2026-11-05T15:40:00Z".parse::<DateTime<Utc>>().unwrap()
        );
        for (field, value) in [
            ("end", json!(null)),
            ("end_utc", json!(now)),
            ("end_offset_seconds", json!(60)),
        ] {
            let mut wrong = raw.clone();
            wrong["instruments"]["AIR_PA_EQ"]["coverage"][field] = value;
            assert!(
                settings
                    .normalize(&quotes, &identity, &"b".repeat(64), &wrong, now)
                    .is_err()
            );
        }
        let future = now + ChronoDuration::seconds(1);
        raw["instruments"]["AIR_PA_EQ"]["coverage"]["end"] =
            json!(future.format("%Y%m%d %H:%M:%S UTC").to_string());
        raw["instruments"]["AIR_PA_EQ"]["coverage"]["end_utc"] = json!(future);
        assert!(
            settings
                .normalize(&quotes, &identity, &"b".repeat(64), &raw, now)
                .is_err()
        );
    }

    #[test]
    fn last_native_candle_needs_a_successor_even_if_request_end_is_later() {
        let (settings, quotes, identity, mut raw, now) = fixture();
        raw["instruments"]["AIR_PA_EQ"]["raw_bars"]
            .as_array_mut()
            .unwrap()
            .pop();
        let result = settings
            .normalize(&quotes, &identity, &"b".repeat(64), &raw, now)
            .unwrap();
        assert_eq!(result.bars.len(), 1);
        assert_eq!(
            result.observed_at,
            "2026-11-05T15:40:00Z".parse::<DateTime<Utc>>().unwrap()
        );
    }

    #[test]
    fn current_bar_raw_contract_configuration_grid_and_history_end_are_bound() {
        let (settings, quotes, identity, raw, now) = fixture();
        for (pointer, value) in [
            ("/status", json!("UNAVAILABLE")),
            ("/provider", json!("YFINANCE")),
            ("/probe_source_sha256", json!("c".repeat(64))),
            ("/quote_config_canonical_sha256", json!("c".repeat(64))),
            ("/crosswalk_canonical_sha256", json!("c".repeat(64))),
            (
                "/instruments/AIR_PA_EQ/historical_end_received",
                json!(false),
            ),
            (
                "/instruments/AIR_PA_EQ/contract/identifiers/ISIN",
                json!("wrong"),
            ),
            (
                "/instruments/AIR_PA_EQ/contract/primary_exchange",
                json!("IBIS"),
            ),
            ("/instruments/AIR_PA_EQ/raw_bars/1/high", json!("1")),
            (
                "/instruments/AIR_PA_EQ/raw_bars/1/date",
                json!("1793892901"),
            ),
        ] {
            let mut changed = raw.clone();
            *changed.pointer_mut(pointer).unwrap() = value;
            assert!(
                settings
                    .normalize(&quotes, &identity, &"b".repeat(64), &changed, now)
                    .is_err(),
                "{pointer}"
            );
        }
    }
}

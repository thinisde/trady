//! Fixed IEX30 holding/market watchdog schedule.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use trady_domain::{Error, Result, require};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IexWatchdogConfig {
    pub config_version: u32,
    pub enabled: bool,
    pub interval_seconds: u32,
    pub quote_max_age_seconds: u32,
    pub symbols: Vec<String>,
    #[serde(default)]
    pub x_source_ids: Vec<String>,
}

impl Default for IexWatchdogConfig {
    fn default() -> Self {
        Self {
            config_version: 1,
            enabled: false,
            interval_seconds: 300,
            quote_max_age_seconds: 10,
            symbols: trady_domain::iex::SYMBOLS
                .iter()
                .map(ToString::to_string)
                .collect(),
            x_source_ids: vec![],
        }
    }
}

impl IexWatchdogConfig {
    pub fn validate(&self) -> Result<()> {
        let symbols: BTreeSet<&str> = self.symbols.iter().map(String::as_str).collect();
        require(
            self.config_version == 1
                && (30..=3600).contains(&self.interval_seconds)
                && (1..=300).contains(&self.quote_max_age_seconds)
                && self.symbols.len() == 30
                && symbols.len() == 30
                && trady_domain::iex::SYMBOLS
                    .iter()
                    .all(|s| symbols.contains(s))
                && self.x_source_ids.len() <= 16
                && self.x_source_ids.iter().all(|id| {
                    !id.is_empty()
                        && id.len() <= 96
                        && id
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
                }),
            "IEX30 watchdog must cover exactly the approved 30 symbols",
        )
    }

    pub fn ticks(&self, now: DateTime<Utc>) -> Result<Vec<WatchdogTick>> {
        self.validate()?;
        require(self.enabled, "IEX30 watchdog is disabled")?;
        let epoch = now.timestamp().div_euclid(i64::from(self.interval_seconds));
        self.symbols
            .iter()
            .map(|symbol| {
                require(
                    trady_domain::iex::SYMBOLS.contains(&symbol.as_str()),
                    "unknown IEX30 symbol",
                )?;
                Ok(WatchdogTick {
                    symbol: symbol.clone(),
                    bucket: epoch,
                    key: format!("iex_watch_{symbol}_{epoch}"),
                })
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WatchdogTick {
    pub symbol: String,
    pub bucket: i64,
    pub key: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WatchdogStatus {
    Healthy,
    Degraded,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IexWatchdogObservation {
    pub bucket: i64,
    pub observed_at: DateTime<Utc>,
    pub status: WatchdogStatus,
    pub holding: bool,
    pub held_symbol: Option<String>,
    pub stream_connected: bool,
    pub stream_reconnects: u64,
    pub last_error: Option<String>,
    pub last_message_at: Option<DateTime<Utc>>,
    pub last_quote_at: Option<DateTime<Utc>>,
    pub quote_age_seconds: Option<u64>,
    pub stale_symbols: Vec<String>,
}

impl IexWatchdogObservation {
    pub fn validate(&self) -> Result<()> {
        let stale: BTreeSet<&str> = self.stale_symbols.iter().map(String::as_str).collect();
        require(
            self.bucket >= 0
                && self.observed_at <= Utc::now() + chrono::Duration::seconds(5)
                && stale.len() == self.stale_symbols.len()
                && self.stale_symbols.len() <= 30
                && self
                    .stale_symbols
                    .iter()
                    .all(|symbol| trady_domain::iex::SYMBOLS.contains(&symbol.as_str()))
                && self
                    .held_symbol
                    .as_ref()
                    .is_none_or(|symbol| trady_domain::iex::SYMBOLS.contains(&symbol.as_str()))
                && self.last_error.as_ref().is_none_or(|error| {
                    error.len() <= 256 && !error.bytes().any(|byte| byte.is_ascii_control())
                })
                && (!self.holding || self.held_symbol.is_some()),
            "invalid IEX watchdog observation",
        )
    }
}

pub fn config_error(config: &IexWatchdogConfig) -> Option<String> {
    config.validate().err().map(|e: Error| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn enabled_watchdog_emits_one_tick_per_symbol() {
        let config = IexWatchdogConfig {
            enabled: true,
            ..IexWatchdogConfig::default()
        };
        let ticks = config
            .ticks("2026-11-02T12:00:00Z".parse().unwrap())
            .unwrap();
        assert_eq!(ticks.len(), 30);
        assert!(ticks.iter().all(|tick| tick.key.starts_with("iex_watch_")));
    }
    #[test]
    fn watchdog_rejects_symbol_drift() {
        let mut config = IexWatchdogConfig::default();
        config.symbols[0] = "SPY".into();
        assert!(config.validate().is_err());
    }

    #[test]
    fn holding_observation_requires_an_approved_held_symbol() {
        let observation = IexWatchdogObservation {
            bucket: 1,
            observed_at: Utc::now(),
            status: WatchdogStatus::Healthy,
            holding: true,
            held_symbol: Some("NVDA".into()),
            stream_connected: true,
            stream_reconnects: 0,
            last_error: None,
            last_message_at: None,
            last_quote_at: None,
            quote_age_seconds: Some(1),
            stale_symbols: vec![],
        };
        observation.validate().unwrap();
        let invalid = IexWatchdogObservation {
            held_symbol: Some("SPY".into()),
            ..observation
        };
        assert!(invalid.validate().is_err());
    }
}

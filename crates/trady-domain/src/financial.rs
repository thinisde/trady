//! Attributed research observations. These types confer no execution authority.

use crate::{Result, require};
use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Provider {
    Sec,
    Fred,
    TwelveData,
    Finnhub,
    AlphaVantage,
    Fmp,
    Gdelt,
    CoinGecko,
    Tiingo,
    Massive,
    Yfinance,
}

impl Provider {
    pub fn credential_name(self) -> Option<&'static str> {
        match self {
            Self::Sec => Some("TRADY_SEC_USER_AGENT"),
            Self::Fred => Some("TRADY_FRED_API_KEY"),
            Self::TwelveData => Some("TRADY_TWELVE_DATA_API_KEY"),
            Self::Finnhub => Some("TRADY_FINNHUB_API_KEY"),
            Self::AlphaVantage => Some("TRADY_ALPHA_VANTAGE_API_KEY"),
            Self::Fmp => Some("TRADY_FMP_API_KEY"),
            Self::CoinGecko => Some("TRADY_COINGECKO_API_KEY"),
            Self::Tiingo => Some("TRADY_TIINGO_API_KEY"),
            Self::Massive => Some("TRADY_MASSIVE_API_KEY"),
            Self::Gdelt | Self::Yfinance => None,
        }
    }
    /// All uses of the same provider credential share one durable allowance.
    pub fn budget_key(self) -> String {
        self.credential_name()
            .unwrap_or(match self {
                Self::Gdelt => "GDELT_PUBLIC_IP",
                Self::Yfinance => "YFINANCE_OFFLINE",
                _ => "UNAVAILABLE",
            })
            .to_owned()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Consumer {
    Display,
    ExternalLlm,
    LocalInference,
    ModelTraining,
    Export,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct DataRights {
    #[serde(default)]
    pub normalized_storage: bool,
    #[serde(default)]
    pub display: bool,
    #[serde(default)]
    pub external_llm: bool,
    #[serde(default)]
    pub local_inference: bool,
    #[serde(default)]
    pub model_training: bool,
    #[serde(default)]
    pub export: bool,
    pub retention_hours: u32,
    pub terms_reference: String,
}
impl DataRights {
    pub fn permits(&self, consumer: Consumer) -> bool {
        self.normalized_storage
            && match consumer {
                Consumer::Display => self.display,
                Consumer::ExternalLlm => self.external_llm,
                Consumer::LocalInference => self.local_inference,
                Consumer::ModelTraining => self.model_training,
                Consumer::Export => self.export,
            }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AccessStatus {
    Unknown,
    Verified,
    Denied,
    Suspended,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capability {
    pub status: AccessStatus,
    pub revision: String,
    pub evidence_reference: String,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub valid_until: Option<DateTime<Utc>>,
    /// Conservative endpoint weight verified for this account, never inferred from HTTP 200.
    pub credit_cost: u32,
}
impl Capability {
    pub fn current(&self, at: DateTime<Utc>) -> bool {
        self.status == AccessStatus::Verified
            && !self.revision.is_empty()
            && !self.evidence_reference.trim().is_empty()
            && self.reviewed_at.is_some_and(|t| t <= at)
            && self.valid_until.is_some_and(|t| t > at)
            && self.credit_cost > 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Adjustment {
    Unadjusted,
    SplitAdjusted,
    TotalReturn,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum FinancialPayload {
    Metric {
        metric: String,
        value: Option<Decimal>,
        period_start: Option<NaiveDate>,
        period_end: NaiveDate,
        vintage: Option<NaiveDate>,
        source_revision: String,
    },
    Filing {
        accession: String,
        form: String,
        filed_on: NaiveDate,
        primary_document: String,
    },
    DailyBar {
        session: NaiveDate,
        open: Decimal,
        high: Decimal,
        low: Decimal,
        close: Decimal,
        volume: Option<Decimal>,
        currency: String,
        adjustment: Adjustment,
    },
    MarketContext {
        asset_id: String,
        price: Option<Decimal>,
        market_cap: Option<Decimal>,
        currency: String,
    },
}

impl FinancialPayload {
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Metric {
                metric,
                period_start,
                period_end,
                source_revision,
                ..
            } => require(
                !metric.is_empty()
                    && metric.len() <= 96
                    && source_revision.len() <= 160
                    && period_start.is_none_or(|t| t <= *period_end),
                "invalid financial metric",
            ),
            Self::Filing {
                accession,
                form,
                primary_document,
                ..
            } => require(
                !accession.is_empty()
                    && accession.len() <= 40
                    && !form.is_empty()
                    && form.len() <= 24
                    && primary_document.len() <= 256
                    && !primary_document.contains(['/', '\\']),
                "invalid filing metadata",
            ),
            Self::DailyBar {
                open,
                high,
                low,
                close,
                volume,
                currency,
                ..
            } => require(
                *open > Decimal::ZERO
                    && *low > Decimal::ZERO
                    && *close > Decimal::ZERO
                    && high >= open
                    && high >= close
                    && high >= low
                    && low <= open
                    && low <= close
                    && volume.is_none_or(|v| v >= Decimal::ZERO)
                    && valid_currency(currency),
                "invalid OHLCV values or currency",
            ),
            Self::MarketContext {
                asset_id,
                price,
                market_cap,
                currency,
            } => require(
                !asset_id.is_empty()
                    && asset_id.len() <= 96
                    && valid_currency(currency)
                    && price.is_none_or(|v| v > Decimal::ZERO)
                    && market_cap.is_none_or(|v| v >= Decimal::ZERO),
                "invalid market context",
            ),
        }
    }
}

pub fn valid_currency(s: &str) -> bool {
    s.len() == 3 && s.bytes().all(|b| b.is_ascii_uppercase())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FinancialObservation {
    pub id: String,
    pub source_id: String,
    pub provider: Provider,
    pub entity_id: String,
    pub external_key: String,
    pub lineage_id: String,
    pub registry_hash: String,
    pub content_hash: String,
    pub parser_version: String,
    pub source_url: String,
    pub unit: String,
    /// Date-only publication is conservatively bounded by the next UTC midnight.
    pub available_at: DateTime<Utc>,
    pub received_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub payload: FinancialPayload,
    pub gaps: Vec<String>,
}
impl FinancialObservation {
    pub fn validate(&self) -> Result<()> {
        self.payload.validate()?;
        let hash = |s: &str| {
            s.len() == 64
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        };
        require(
            hash(&self.id)
                && hash(&self.registry_hash)
                && hash(&self.content_hash)
                && crate::InstrumentId::new(self.entity_id.clone()).is_ok()
                && crate::InstrumentId::new(self.source_id.clone()).is_ok()
                && crate::InstrumentId::new(self.lineage_id.clone()).is_ok()
                && !self.parser_version.is_empty()
                && self.parser_version.len() <= 64,
            "invalid financial observation identity",
        )?;
        require(
            self.available_at <= self.received_at
                && self.expires_at > self.received_at
                && self.external_key.len() <= 256
                && !self.external_key.is_empty()
                && self.source_url.starts_with("https://")
                && !self.source_url.contains(['?', '#', '@'])
                && self.source_url.len() <= 2048
                && self.unit.len() <= 64
                && self.gaps.len() <= 16
                && self.gaps.iter().all(|s| s.len() <= 256),
            "invalid financial observation envelope",
        )?;
        if let FinancialPayload::DailyBar { session, .. } = &self.payload {
            require(
                *session < self.received_at.date_naive(),
                "forming daily bar is unavailable",
            )?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DataStatus {
    Ok,
    Partial,
    Stale,
    Error,
    Denied,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FinancialSnapshot {
    pub schema_version: u32,
    pub manifest_hash: String,
    pub source_id: String,
    pub entity_id: String,
    pub decision_at: DateTime<Utc>,
    pub consumer: Consumer,
    pub status: DataStatus,
    pub coverage: Option<FinancialCoverage>,
    pub observations: Vec<FinancialObservation>,
    pub gaps: Vec<String>,
    pub entry_authorized: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FinancialCoverage {
    pub start: NaiveDate,
    pub end: NaiveDate,
    pub complete: bool,
}

/// Deterministic signals retain their exact dependencies and data-quality masks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FinancialSignals {
    pub snapshot: FinancialSnapshot,
    pub last_close: Option<Decimal>,
    pub return_fraction: Option<Decimal>,
    pub mean_close: Option<Decimal>,
    pub complete_bars: usize,
    pub feature_schema: String,
}

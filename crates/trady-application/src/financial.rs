//! Shared bounded collection, current permissions, and point-in-time signal reads.

use crate::{research::valid_key, research_jobs::digest};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use trady_domain::{Error, Result, financial::*, require};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetLimits {
    pub per_minute: u32,
    pub per_day: u32,
    pub per_month: u32,
}
impl BudgetLimits {
    pub fn validate(&self) -> Result<()> {
        require(
            self.per_minute > 0
                && self.per_day >= self.per_minute
                && self.per_month >= self.per_day
                && self.per_month <= 1_000_000,
            "invalid reviewed financial credit limits",
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderAccount {
    pub provider: Provider,
    pub enabled: bool,
    pub limits: BudgetLimits,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "endpoint",
    rename_all = "SCREAMING_SNAKE_CASE",
    deny_unknown_fields
)]
pub enum DataRequest {
    SecConcept {
        cik: String,
        taxonomy: String,
        concept: String,
    },
    SecSubmissions {
        cik: String,
    },
    FredObservations {
        series_id: String,
        vintage: Option<NaiveDate>,
    },
    TwelveDaily {
        symbol: String,
        exchange: String,
    },
    FmpIncome {
        symbol: String,
        metric: String,
    },
    CoinGeckoMarkets {
        coin_id: String,
    },
    TiingoDaily {
        symbol: String,
    },
    MassiveDaily {
        symbol: String,
    },
    YfinanceImport,
}
impl DataRequest {
    pub fn provider(&self) -> Provider {
        match self {
            Self::SecConcept { .. } | Self::SecSubmissions { .. } => Provider::Sec,
            Self::FredObservations { .. } => Provider::Fred,
            Self::TwelveDaily { .. } => Provider::TwelveData,
            Self::FmpIncome { .. } => Provider::Fmp,
            Self::CoinGeckoMarkets { .. } => Provider::CoinGecko,
            Self::TiingoDaily { .. } => Provider::Tiingo,
            Self::MassiveDaily { .. } => Provider::Massive,
            Self::YfinanceImport => Provider::Yfinance,
        }
    }
    pub fn validate(&self) -> Result<()> {
        let symbol = |s: &str| {
            !s.is_empty()
                && s.len() <= 64
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        };
        let cik =
            |s: &str| s.len() == 10 && s.bytes().all(|b| b.is_ascii_digit()) && s != "0000000000";
        let valid = match self {
            Self::SecConcept {
                cik: id,
                taxonomy,
                concept,
            } => cik(id) && valid_key(taxonomy) && valid_key(concept),
            Self::SecSubmissions { cik: id } => cik(id),
            Self::FredObservations { series_id, .. } => symbol(series_id),
            Self::TwelveDaily {
                symbol: s,
                exchange,
            } => symbol(s) && symbol(exchange),
            Self::FmpIncome { symbol: s, metric } => symbol(s) && valid_key(metric),
            Self::CoinGeckoMarkets { coin_id } => valid_key(coin_id),
            Self::TiingoDaily { symbol: s } | Self::MassiveDaily { symbol: s } => symbol(s),
            Self::YfinanceImport => true,
        };
        require(valid, "invalid reviewed financial endpoint parameters")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FinancialSource {
    pub id: String,
    pub enabled: bool,
    /// Stable reviewed identity; a filer CIK is not an instrument mapping.
    pub entity_id: String,
    pub mapping_reference: String,
    pub lineage_id: String,
    pub request: DataRequest,
    pub unit: String,
    pub currency: Option<String>,
    pub adjustment: Adjustment,
    pub max_records: usize,
    pub interval_seconds: u32,
    pub freshness_seconds: u32,
    pub capability: Capability,
    pub rights: DataRights,
}
impl FinancialSource {
    pub fn validate(&self) -> Result<()> {
        self.request.validate()?;
        require(
            valid_key(&self.id)
                && valid_key(&self.entity_id)
                && valid_key(&self.lineage_id)
                && self.mapping_reference.len() <= 512
                && self.unit.len() <= 64
                && !self.unit.is_empty()
                && self.currency.as_deref().is_none_or(valid_currency)
                && (1..=500).contains(&self.max_records)
                && (60..=604800).contains(&self.interval_seconds)
                && (60..=31536000).contains(&self.freshness_seconds)
                && (1..=8760).contains(&self.rights.retention_hours)
                && self.rights.terms_reference.len() <= 512
                && self.capability.revision.len() <= 96
                && self.capability.evidence_reference.len() <= 512
                && self.capability.credit_cost <= 10000,
            "invalid financial source bounds",
        )?;
        if self.enabled {
            require(
                !self.mapping_reference.trim().is_empty(),
                "enabled source needs reviewed identity mapping",
            )?;
        }
        if matches!(
            self.request,
            DataRequest::TwelveDaily { .. }
                | DataRequest::TiingoDaily { .. }
                | DataRequest::MassiveDaily { .. }
        ) {
            require(
                self.currency.is_some()
                    && self.unit == "price"
                    && self.adjustment == Adjustment::Unadjusted,
                "daily price adapters require explicit currency and unadjusted price semantics",
            )?;
        }
        Ok(())
    }
    pub fn registry_hash(&self) -> Result<String> {
        digest(self)
    }
    pub fn accepts(&self, o: &FinancialObservation, window: &DataWindow) -> bool {
        if o.provider != self.request.provider()
            || o.unit != self.unit
            || o.lineage_id != self.lineage_id
        {
            return false;
        }
        let in_window = |day: NaiveDate| day >= window.start && day <= window.end;
        match (&self.request, &o.payload) {
            (
                DataRequest::SecConcept { concept, .. },
                FinancialPayload::Metric {
                    metric, period_end, ..
                },
            ) => concept == metric && in_window(*period_end),
            (
                DataRequest::FredObservations { series_id, .. },
                FinancialPayload::Metric {
                    metric, period_end, ..
                },
            ) => series_id == metric && in_window(*period_end),
            (
                DataRequest::FmpIncome {
                    metric: expected, ..
                },
                FinancialPayload::Metric {
                    metric, period_end, ..
                },
            ) => expected == metric && in_window(*period_end),
            (DataRequest::SecSubmissions { .. }, FinancialPayload::Filing { filed_on, .. }) => {
                in_window(*filed_on)
            }
            (
                DataRequest::TwelveDaily { .. }
                | DataRequest::TiingoDaily { .. }
                | DataRequest::MassiveDaily { .. }
                | DataRequest::YfinanceImport,
                FinancialPayload::DailyBar {
                    session,
                    currency,
                    adjustment,
                    ..
                },
            ) => {
                in_window(*session)
                    && self.currency.as_deref() == Some(currency.as_str())
                    && self.adjustment == *adjustment
            }
            (
                DataRequest::CoinGeckoMarkets { coin_id },
                FinancialPayload::MarketContext {
                    asset_id, currency, ..
                },
            ) => coin_id == asset_id && self.currency.as_deref() == Some(currency.as_str()),
            _ => false,
        }
    }
    pub fn usable(&self, now: DateTime<Utc>) -> bool {
        self.enabled
            && self.capability.current(now)
            && self.rights.normalized_storage
            && !self.rights.terms_reference.trim().is_empty()
            && !self.mapping_reference.trim().is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FinancialConfig {
    pub config_version: u32,
    pub research_only: bool,
    pub automatic_paid_fallback: bool,
    pub accounts: Vec<ProviderAccount>,
    pub sources: Vec<FinancialSource>,
}
impl FinancialConfig {
    pub fn validate(&self) -> Result<()> {
        require(
            self.config_version == 1
                && self.research_only
                && !self.automatic_paid_fallback
                && self.accounts.len() <= 11
                && self.sources.len() <= 64,
            "unsupported financial registry or execution authority",
        )?;
        let mut providers = BTreeSet::new();
        for account in &self.accounts {
            account.limits.validate()?;
            require(
                providers.insert(account.provider),
                "duplicate credential budget account",
            )?;
        }
        let mut ids = BTreeSet::new();
        for source in &self.sources {
            source.validate()?;
            require(
                ids.insert(&source.id) && providers.contains(&source.request.provider()),
                "duplicate financial source or missing provider account",
            )?;
        }
        Ok(())
    }
    pub fn account(&self, source: &FinancialSource) -> Result<&ProviderAccount> {
        self.accounts
            .iter()
            .find(|a| a.provider == source.request.provider())
            .ok_or_else(|| Error("provider account unavailable".into()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DataWindow {
    pub start: NaiveDate,
    pub end: NaiveDate,
}
impl DataWindow {
    pub fn validate(&self, now: DateTime<Utc>) -> Result<()> {
        require(
            self.start <= self.end
                && self.end < now.date_naive()
                && self.end.signed_duration_since(self.start).num_days() <= 366,
            "financial window must be bounded and end before today",
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionLease {
    pub request_id: String,
    pub source_id: String,
    pub registry_hash: String,
    pub window: DataWindow,
}
#[derive(Debug, Clone)]
pub struct FinancialPage {
    pub observations: Vec<FinancialObservation>,
    pub complete: bool,
    pub gaps: Vec<String>,
}
#[derive(Debug, Clone)]
pub struct FinancialFailure {
    pub status: DataStatus,
    pub code: &'static str,
    pub retry_at: Option<DateTime<Utc>>,
}
pub trait FinancialProvider {
    fn fetch(
        &mut self,
        source: &FinancialSource,
        window: &DataWindow,
        now: DateTime<Utc>,
    ) -> std::result::Result<FinancialPage, FinancialFailure>;
}
pub trait FinancialRepository {
    /// Synchronizes the complete reviewed registry, disabling sources removed from it.
    fn sync_registry(&mut self, config: &FinancialConfig) -> Result<()>;
    fn reserve(
        &mut self,
        source: &FinancialSource,
        account: &ProviderAccount,
        window: &DataWindow,
        now: DateTime<Utc>,
    ) -> Result<Option<CollectionLease>>;
    fn finish(
        &mut self,
        lease: &CollectionLease,
        page: &FinancialPage,
        now: DateTime<Utc>,
    ) -> Result<usize>;
    fn fail(
        &mut self,
        lease: &CollectionLease,
        failure: &FinancialFailure,
        now: DateTime<Utc>,
    ) -> Result<()>;
    fn read(
        &mut self,
        source_id: &str,
        cutoff: DateTime<Utc>,
        consumer: Consumer,
        limit: usize,
        now: DateTime<Utc>,
    ) -> Result<FinancialSnapshot>;
}

#[derive(Debug, Serialize)]
pub struct FinancialCollectionReport {
    pub source_id: String,
    pub status: DataStatus,
    pub stored: usize,
    pub gaps: Vec<String>,
}
pub fn collect(
    source: &FinancialSource,
    account: &ProviderAccount,
    window: &DataWindow,
    provider: &mut impl FinancialProvider,
    repository: &mut impl FinancialRepository,
    now: DateTime<Utc>,
) -> Result<FinancialCollectionReport> {
    source.validate()?;
    account.limits.validate()?;
    window.validate(now)?;
    let report = |status, stored, gaps| FinancialCollectionReport {
        source_id: source.id.clone(),
        status,
        stored,
        gaps,
    };
    if !account.enabled || !source.usable(now) {
        return Ok(report(
            DataStatus::Denied,
            0,
            vec!["source_account_capability_or_storage_not_approved".into()],
        ));
    }
    require(
        account.provider == source.request.provider(),
        "provider account mismatch",
    )?;
    let Some(lease) = repository.reserve(source, account, window, now)? else {
        return Ok(report(
            DataStatus::Unavailable,
            0,
            vec!["budget_retry_interval_or_active_lease".into()],
        ));
    };
    match provider.fetch(source, window, now) {
        Ok(page) => {
            let received_at = page
                .observations
                .iter()
                .map(|o| o.received_at)
                .max()
                .unwrap_or(now);
            let valid = page.observations.len() <= source.max_records
                && page.observations.iter().all(|o| {
                    o.validate().is_ok()
                        && o.source_id == source.id
                        && o.provider == source.request.provider()
                        && o.entity_id == source.entity_id
                        && o.registry_hash == lease.registry_hash
                        && o.received_at >= now
                        && o.received_at == received_at
                        && received_at < now + Duration::seconds(90)
                        && source.accepts(o, window)
                });
            if !valid {
                let failure = FinancialFailure {
                    status: DataStatus::Error,
                    code: "financial_page_validation_failed",
                    retry_at: None,
                };
                repository.fail(&lease, &failure, now)?;
                return Ok(report(failure.status, 0, vec![failure.code.into()]));
            }
            let count = repository.finish(&lease, &page, received_at)?;
            Ok(report(
                if page.observations.is_empty() {
                    DataStatus::Unavailable
                } else if page.complete && page.gaps.is_empty() {
                    DataStatus::Ok
                } else {
                    DataStatus::Partial
                },
                count,
                page.gaps,
            ))
        }
        Err(failure) => {
            repository.fail(&lease, &failure, now)?;
            Ok(report(failure.status, 0, vec![failure.code.into()]))
        }
    }
}

/// Runtime replay never backdates newly downloaded historical observations.
pub fn snapshot(
    source: &FinancialSource,
    mut observations: Vec<FinancialObservation>,
    cutoff: DateTime<Utc>,
    consumer: Consumer,
    coverage_complete: bool,
    now: DateTime<Utc>,
) -> Result<FinancialSnapshot> {
    source.validate()?;
    require(cutoff <= now, "future financial snapshot is forbidden")?;
    let mut gaps = vec![];
    let denied = !source.usable(now)
        || !source.capability.current(cutoff)
        || !source.rights.permits(consumer);
    let hash = source.registry_hash()?;
    observations.retain(|o| {
        !denied
            && o.validate().is_ok()
            && o.source_id == source.id
            && o.entity_id == source.entity_id
            && o.provider == source.request.provider()
            && o.unit == source.unit
            && o.lineage_id == source.lineage_id
            && o.registry_hash == hash
            && o.received_at <= cutoff
            && o.available_at <= cutoff
            && o.expires_at > now
    });
    observations
        .sort_by(|a, b| (a.available_at, &a.external_key).cmp(&(b.available_at, &b.external_key)));
    let stale = observations
        .iter()
        .map(|o| match &o.payload {
            FinancialPayload::DailyBar { session, .. } => session
                .succ_opt()
                .and_then(|d| d.and_hms_opt(0, 0, 0))
                .map(|d| d.and_utc())
                .unwrap_or(o.available_at),
            _ => o.available_at,
        })
        .max()
        .is_some_and(|t| cutoff - t > Duration::seconds(i64::from(source.freshness_seconds)))
        || observations
            .iter()
            .map(|o| o.received_at)
            .max()
            .is_some_and(|t| cutoff - t > Duration::seconds(i64::from(source.freshness_seconds)));
    let status = if denied {
        gaps.push("current_capability_mapping_or_consumer_rights_denied".into());
        DataStatus::Denied
    } else if observations.is_empty() {
        gaps.push("no_eligible_observations".into());
        DataStatus::Unavailable
    } else if stale {
        gaps.push("snapshot_stale".into());
        DataStatus::Stale
    } else if !coverage_complete || observations.iter().any(|o| !o.gaps.is_empty()) {
        gaps.push("coverage_partial".into());
        DataStatus::Partial
    } else {
        DataStatus::Ok
    };
    let manifest_hash = digest(&(&hash, cutoff, consumer, &observations, status))?;
    Ok(FinancialSnapshot {
        schema_version: 1,
        manifest_hash,
        source_id: source.id.clone(),
        entity_id: source.entity_id.clone(),
        decision_at: cutoff,
        consumer,
        status,
        coverage: None,
        observations,
        gaps,
        entry_authorized: false,
    })
}

pub fn signals(snapshot: FinancialSnapshot) -> Result<FinancialSignals> {
    require(
        snapshot.schema_version == 1 && !snapshot.entry_authorized,
        "invalid financial signal authority or schema",
    )?;
    let mut bars: Vec<_> = snapshot
        .observations
        .iter()
        .filter_map(|o| {
            if let FinancialPayload::DailyBar {
                session,
                close,
                currency,
                adjustment,
                ..
            } = &o.payload
            {
                Some((*session, *close, currency, *adjustment))
            } else {
                None
            }
        })
        .collect();
    bars.sort_by_key(|b| b.0);
    let compatible = bars
        .windows(2)
        .all(|w| w[0].0 < w[1].0 && w[0].2 == w[1].2 && w[0].3 == w[1].3);
    let usable = snapshot.status == DataStatus::Ok && compatible;
    let last_close = usable.then(|| bars.last().map(|b| b.1)).flatten();
    let return_fraction = if usable && bars.len() >= 2 {
        bars.last()
            .unwrap()
            .1
            .checked_div(bars[0].1)
            .and_then(|v| v.checked_sub(Decimal::ONE))
    } else {
        None
    };
    let sum = bars
        .iter()
        .try_fold(Decimal::ZERO, |a, b| a.checked_add(b.1));
    let mean_close = if usable && !bars.is_empty() {
        sum.and_then(|v| v.checked_div(Decimal::from(bars.len() as u64)))
    } else {
        None
    };
    Ok(FinancialSignals {
        complete_bars: bars.len(),
        snapshot,
        last_close,
        return_fraction,
        mean_close,
        feature_schema: "trady-financial-context-v1".into(),
    })
}

/// Numerical services consume only separately permitted, schema-labelled inputs.
/// The model deployment still has to approve this exact feature schema.
pub fn model_features(data: &FinancialSignals) -> Result<trady_domain::perpetual::FeatureSnapshot> {
    use trady_domain::perpetual::{FeatureLineage, FeatureSnapshot, FeatureValue};
    let recalculated = signals(data.snapshot.clone())?;
    require(
        recalculated == *data,
        "financial derived values or feature schema changed",
    )?;
    require(
        data.snapshot.status == DataStatus::Ok
            && matches!(
                data.snapshot.consumer,
                Consumer::LocalInference | Consumer::ModelTraining
            )
            && !data.snapshot.observations.is_empty()
            && data.snapshot.observations.len() <= 32,
        "model financial features unavailable, unlicensed or beyond lineage bounds",
    )?;
    let lineage: Vec<_> = data
        .snapshot
        .observations
        .iter()
        .map(|o| FeatureLineage {
            source_id: o.source_id.clone(),
            source_revision: o.content_hash.clone(),
            evidence_id: o.id.clone(),
            observed_at: o.available_at,
            available_at: o.received_at,
        })
        .collect();
    let mut features = vec![];
    for (name, value) in [
        ("financial_last_close", data.last_close),
        ("financial_window_return", data.return_fraction),
        ("financial_mean_close", data.mean_close),
    ] {
        if let Some(value) = value {
            let value = value
                .to_string()
                .parse::<f64>()
                .map_err(|_| Error("financial tensor conversion failed".into()))?;
            require(value.is_finite(), "nonfinite financial tensor")?;
            features.push(FeatureValue {
                name: name.into(),
                value,
                lineage: lineage.clone(),
            });
        }
    }
    require(
        !features.is_empty(),
        "financial features missing; no zero imputation",
    )?;
    Ok(FeatureSnapshot {
        snapshot_id: data.snapshot.manifest_hash.clone(),
        schema_version: data.feature_schema.clone(),
        as_of: data.snapshot.decision_at,
        valid_until: data
            .snapshot
            .observations
            .iter()
            .map(|o| o.expires_at)
            .min()
            .unwrap()
            .min(data.snapshot.decision_at + Duration::minutes(5)),
        features,
    })
}

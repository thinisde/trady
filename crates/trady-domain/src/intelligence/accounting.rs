//! Comparable financial facts and empirical research ranks, without a risk score.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FinancialConcept {
    NetIncome,
    OperatingCashFlow,
    Assets,
    TradeReceivables,
    Revenue,
    Inventory,
    DilutedWeightedAverageShares,
    OutstandingShares,
    Impairment,
    Restructuring,
    PensionOneOff,
    AcquisitionOneOff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FinancialPeriodKind {
    Instant,
    Quarter,
    YearToDate,
    Annual,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FinancialPeriod {
    pub fiscal_label: String,
    pub kind: FinancialPeriodKind,
    pub start: Option<NaiveDate>,
    pub end: NaiveDate,
    pub fiscal_weeks: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FinancialUnit {
    /// "currency" or "shares"; scale converts the as-filed amount to base units.
    pub measure: String,
    pub currency: Option<String>,
    pub scale: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShareCountAdjustment {
    pub factor: Decimal,
    pub basis_date: NaiveDate,
    pub available_at: DateTime<Utc>,
    pub source_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComparableFinancialFact {
    pub fact_id: String,
    pub issuer_id: String,
    pub concept: FinancialConcept,
    pub label_as_filed: String,
    pub taxonomy_tag: Option<String>,
    pub crosswalk_revision: Option<String>,
    pub period: FinancialPeriod,
    pub unit: FinancialUnit,
    pub accounting_basis: String,
    pub consolidated_scope: String,
    pub value: Option<Decimal>,
    pub missing_reason: Option<String>,
    pub filing_accession: String,
    pub filed_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    pub revision_of: Option<String>,
    pub source_ref: Option<String>,
    pub source_text_offset: Option<u64>,
    /// Required even for a verified factor of one when comparing share counts.
    pub share_adjustment: Option<ShareCountAdjustment>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AccountingMetric {
    AccrualsToAverageAssets,
    CashConversion,
    ReceivablesRevenueGrowthGap,
    InventoryRevenueGrowthGap,
    DilutedWeightedShareGrowth,
    OutstandingShareGrowth,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountingCohort {
    pub industry_id: String,
    pub model_revision: String,
    pub inventory_relevant: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountingComparabilityIssue {
    pub metrics: Vec<AccountingMetric>,
    pub reason: String,
    pub available_at: DateTime<Utc>,
    pub source_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountingPeerObservation {
    pub issuer_id: String,
    pub industry_id: String,
    pub cohort_model_revision: String,
    pub metric: AccountingMetric,
    pub formula_revision: String,
    pub crosswalk_revision: String,
    pub period_kind: FinancialPeriodKind,
    pub fiscal_weeks: Option<u16>,
    pub period_end: NaiveDate,
    pub accounting_basis: String,
    pub consolidated_scope: String,
    pub comparable: bool,
    pub value: Option<Decimal>,
    pub missing_reason: Option<String>,
    pub available_at: DateTime<Utc>,
    pub source_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountingInput {
    pub issuer_id: String,
    pub current_period: FinancialPeriod,
    pub comparison_period: FinancialPeriod,
    pub crosswalk_revision: String,
    pub accounting_basis: String,
    pub consolidated_scope: String,
    pub cohort: AccountingCohort,
    /// Absolute materiality floor in the ratio denominator's normalized base units.
    pub minimum_denominator: Decimal,
    pub minimum_cohort_size: u32,
    pub facts: Vec<ComparableFinancialFact>,
    pub comparability_issues: Vec<AccountingComparabilityIssue>,
    pub peers: Vec<AccountingPeerObservation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnomalySignal {
    pub metric: AccountingMetric,
    pub formula_revision: String,
    pub formula: String,
    pub value: Option<Decimal>,
    pub fact_ids: Vec<String>,
    pub abstention_reasons: Vec<String>,
    /// Midrank in the supplied, comparable industry cohort. Not a probability.
    pub empirical_percentile: Option<Decimal>,
    pub cohort_size: u32,
    pub rank_abstention_reason: Option<String>,
    pub cohort_source_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FinancialFactRevision {
    pub original_fact_id: String,
    pub replacement_fact_id: String,
    pub available_at: DateTime<Utc>,
    pub source_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountingAssessment {
    pub issuer_id: String,
    pub as_of: DateTime<Utc>,
    pub version: String,
    pub crosswalk_revision: String,
    pub current_period: FinancialPeriod,
    pub comparison_period: FinancialPeriod,
    pub cohort: AccountingCohort,
    pub signals: Vec<AnomalySignal>,
    pub restatements: Vec<FinancialFactRevision>,
    pub one_off_disclosures: Vec<ComparableFinancialFact>,
    pub reasons: Vec<String>,
}

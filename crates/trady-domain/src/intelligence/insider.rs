//! As-filed Section 16 evidence. These records convey research context only.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OwnershipForm {
    Form3,
    Form4,
    Form5,
    Form3Amendment,
    Form4Amendment,
    Form5Amendment,
}

impl OwnershipForm {
    pub fn is_amendment(self) -> bool {
        matches!(
            self,
            Self::Form3Amendment | Self::Form4Amendment | Self::Form5Amendment
        )
    }

    pub fn sec_name(self) -> &'static str {
        match self {
            Self::Form3 => "3",
            Self::Form4 => "4",
            Self::Form5 => "5",
            Self::Form3Amendment => "3/A",
            Self::Form4Amendment => "4/A",
            Self::Form5Amendment => "5/A",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OwnershipTable {
    NonDerivative,
    Derivative,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PlanAnnotation {
    /// A disclosure describes a plan; this does not establish the person's intent.
    DisclosedPlan,
    /// Requires an explicit review of the applicable disclosures, not an unchecked box.
    NoPlanDisclosedAfterReview,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FilingCoverage {
    CompleteForDeclaredScope,
    Partial,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FilingFootnote {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportingOwner {
    pub owner_id: String,
    pub name_as_filed: Option<String>,
    pub is_director: Option<bool>,
    pub is_officer: Option<bool>,
    pub is_ten_percent_owner: Option<bool>,
    pub is_other: Option<bool>,
    pub officer_title: Option<String>,
    pub other_relationship: Option<String>,
    pub missing_reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InsiderLineRef {
    pub accession: String,
    pub line_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InsiderSplitAdjustment {
    pub quantity_factor: Decimal,
    pub price_factor: Decimal,
    pub basis_date: NaiveDate,
    pub available_at: DateTime<Utc>,
    pub source_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InsiderTransaction {
    /// Content-derived identity, not a row index.
    pub line_id: String,
    /// Same economic transaction across duplicate rows/filings; includes owner attribution.
    pub economic_id: String,
    pub owner_ids: Vec<String>,
    pub table: OwnershipTable,
    pub security_title: String,
    pub transaction_date: NaiveDate,
    pub transaction_code: String,
    pub acquired_or_disposed: Option<String>,
    pub shares: Option<Decimal>,
    pub quantity_unit: String,
    pub price_per_unit: Option<Decimal>,
    pub transaction_currency: Option<String>,
    pub direct_or_indirect: Option<String>,
    pub nature_of_ownership: Option<String>,
    pub shares_owned_following: Option<Decimal>,
    pub conversion_or_exercise_price: Option<Decimal>,
    pub exercise_date: Option<NaiveDate>,
    pub expiration_date: Option<NaiveDate>,
    pub underlying_security: Option<String>,
    pub underlying_shares: Option<Decimal>,
    pub footnote_ids: Vec<String>,
    pub plan_annotation: PlanAnnotation,
    /// Human/structured review must distinguish open-market cash from private transfers.
    pub cash_market_confirmed: Option<bool>,
    pub review_reference: Option<String>,
    pub split_adjustment: Option<InsiderSplitAdjustment>,
    /// An amendment replaces only this explicitly linked original line.
    pub supersedes: Option<InsiderLineRef>,
    pub source_ref: String,
    pub source_byte_start: u64,
    pub source_byte_end: u64,
    pub missing_reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InsiderHolding {
    pub line_id: String,
    pub owner_ids: Vec<String>,
    pub table: OwnershipTable,
    pub security_title: String,
    pub shares_owned: Option<Decimal>,
    pub direct_or_indirect: Option<String>,
    pub nature_of_ownership: Option<String>,
    pub conversion_or_exercise_price: Option<Decimal>,
    pub exercise_date: Option<NaiveDate>,
    pub expiration_date: Option<NaiveDate>,
    pub underlying_security: Option<String>,
    pub underlying_shares: Option<Decimal>,
    pub footnote_ids: Vec<String>,
    pub source_ref: String,
    pub source_byte_start: u64,
    pub source_byte_end: u64,
    pub missing_reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnershipFiling {
    pub issuer_cik: String,
    pub issuer_name_as_filed: Option<String>,
    pub issuer_trading_symbol: Option<String>,
    pub accession: String,
    pub form: OwnershipForm,
    pub filed_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    pub period_of_report: NaiveDate,
    pub original_submission_date: Option<NaiveDate>,
    pub amendment_of: Option<String>,
    pub plan_checkbox: Option<bool>,
    pub owners: Vec<ReportingOwner>,
    pub transactions: Vec<InsiderTransaction>,
    pub holdings: Vec<InsiderHolding>,
    pub footnotes: Vec<FilingFootnote>,
    pub remarks: Option<String>,
    pub source_ref: String,
    pub source_content_hash: String,
    pub missing_reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InsiderInput {
    pub issuer_cik: String,
    /// Inclusive public-availability and transaction-date interval, declared before assessment.
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    pub reporting_currency: String,
    pub minimum_cluster_value: Decimal,
    pub minimum_distinct_buyers: u32,
    pub coverage: FilingCoverage,
    pub coverage_reason: Option<String>,
    pub filings: Vec<OwnershipFiling>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InsiderTransactionKind {
    ReviewedCashPurchase,
    ReviewedCashSale,
    PlanDescribed,
    OptionExercise,
    TaxWithholding,
    Award,
    Gift,
    Derivative,
    UncertainPurchaseOrSale,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InsiderRowAssessment {
    pub line: InsiderLineRef,
    pub economic_id: String,
    pub owner_ids: Vec<String>,
    pub available_at: DateTime<Utc>,
    pub transaction_date: NaiveDate,
    pub kind: InsiderTransactionKind,
    pub as_filed_notional: Option<Decimal>,
    pub notional_currency: Option<String>,
    pub quantity_unit: String,
    pub plan_annotation: PlanAnnotation,
    pub included_in_totals: bool,
    pub reasons: Vec<String>,
    pub source_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InsiderAssessment {
    pub issuer_cik: String,
    pub as_of: DateTime<Utc>,
    pub version: String,
    pub reporting_currency: String,
    pub coverage: FilingCoverage,
    /// Observed subtotals, never proof that uncovered/exempt insiders had zero trades.
    pub observed_purchase_value: Option<Decimal>,
    pub observed_sale_value: Option<Decimal>,
    pub observed_net_value: Option<Decimal>,
    pub distinct_qualifying_buyers: Option<u32>,
    pub purchase_cluster: Option<bool>,
    pub role_breadth: Vec<String>,
    pub rows: Vec<InsiderRowAssessment>,
    pub reasons: Vec<String>,
}

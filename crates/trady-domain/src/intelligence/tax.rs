//! Separately permissioned tax inputs and identity-free, reporting-only illustrations.
//! No jurisdiction, rate, exemption, residence or lot basis is inferred.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::{Currency, InstrumentId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaxContext {
    pub jurisdiction: String,
    pub tax_year: i32,
    pub account_wrapper: String,
    pub broker_entity: String,
    pub tax_currency: Currency,
    pub residence_owner_confirmed: bool,
    pub account_treatment_owner_confirmed: bool,
    pub confirmed_at: DateTime<Utc>,
    pub statement_as_of: DateTime<Utc>,
    pub statement_reference: String,
    pub valid_until: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaxLotMethod {
    Fifo,
    SpecificIdentification,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaxLossTreatment {
    NoOffsets,
    GainsOnly,
    GainsThenDividends,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaxAllowanceAllocation {
    GainsOnly,
    GainsThenDividends,
    DividendsThenGains,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaxRateRange {
    pub lower: Decimal,
    pub central: Decimal,
    pub upper: Decimal,
}

/// This initial calculator supports explicitly approved linear rate scenarios.
/// Other treatments must remain unavailable, rather than be approximated silently.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectiveRuleSet {
    pub version: String,
    pub jurisdiction: String,
    pub tax_year: i32,
    pub account_wrapper: String,
    pub broker_entity: String,
    pub tax_currency: Currency,
    pub effective_from: DateTime<Utc>,
    pub effective_until: DateTime<Utc>,
    pub source_published_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    pub owner_reviewed_at: DateTime<Utc>,
    pub source_reference: String,
    pub owner_approved: bool,
    pub linear_scenario_supported: bool,
    pub lot_method: TaxLotMethod,
    pub specific_identification_permitted: bool,
    pub gains_rate: TaxRateRange,
    pub dividend_rate: TaxRateRange,
    pub loss_treatment: TaxLossTreatment,
    pub allowance_allocation: TaxAllowanceAllocation,
    pub carryforward_permitted: bool,
    pub acquisition_fees_in_basis: bool,
    pub sale_costs_deductible: bool,
    pub withholding_credit_permitted: bool,
    pub excess_withholding_refundable: bool,
    /// This implementation requires documented acquisition/disposal-date FX.
    pub acquisition_disposal_date_fx_approved: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaxFxRate {
    pub from: Currency,
    pub to: Currency,
    pub rate: Decimal,
    pub valuation_date: NaiveDate,
    pub source_published_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    pub reference: String,
    pub owner_reviewed: bool,
    /// Explicitly labeled FX assumptions are permitted for future illustrations only.
    pub illustrative_assumption: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CostBasisLot {
    pub lot_id: String,
    pub instrument_id: InstrumentId,
    pub acquired_at: DateTime<Utc>,
    /// Owner-reviewed legally required valuation date, independent of UTC date.
    pub tax_valuation_date: NaiveDate,
    /// Actual acquisition quantity before the separately supplied confirmed splits.
    pub quantity: Decimal,
    pub unit_cost: Decimal,
    pub acquisition_fees: Decimal,
    pub currency: Currency,
    pub acquisition_fx: Option<TaxFxRate>,
    pub broker_confirmed: bool,
    pub available_at: DateTime<Utc>,
    pub statement_reference: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaxSplit {
    pub action_id: String,
    pub instrument_id: InstrumentId,
    pub effective_at: DateTime<Utc>,
    pub numerator: Decimal,
    pub denominator: Decimal,
    pub broker_confirmed: bool,
    pub available_at: DateTime<Utc>,
    pub reference: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaxWithholding {
    /// Same broker deduction keeps one ID across cash and annual statement imports.
    pub event_id: String,
    pub related_disposal_id: Option<String>,
    pub amount_tax_currency: Decimal,
    pub paid_at: DateTime<Utc>,
    pub available_at: DateTime<Utc>,
    pub reference: String,
    pub broker_confirmed: bool,
    pub illustrative_assumption: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnnualTaxState {
    /// Reconciled income excluding all proposed disposals in this comparison.
    pub gross_stock_gains: Option<Decimal>,
    pub stock_losses: Option<Decimal>,
    pub gross_dividends: Option<Decimal>,
    pub confirmed_allowance: Option<Decimal>,
    pub confirmed_loss_carryforward: Option<Decimal>,
    pub unknown_reason: Option<String>,
    pub owner_confirmed: bool,
    pub available_at: DateTime<Utc>,
    pub statement_reference: String,
    /// May repeat the same deduction from different broker reports; deduplicated by ID.
    pub withholding: Vec<TaxWithholding>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaxCashBasis {
    GrossBeforeWithholding,
    NetAfterWithholding,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaxCashObservation {
    pub amount_tax_currency: Decimal,
    pub basis: TaxCashBasis,
    pub available_at: DateTime<Utc>,
    pub reference: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaxDisposalScenario {
    pub disposal_id: String,
    pub disposed_at: DateTime<Utc>,
    pub tax_valuation_date: NaiveDate,
    pub quantity: Decimal,
    pub unit_proceeds: Decimal,
    /// Fees, slippage and FX charges are explicitly supplied in proceeds currency.
    pub sale_fees: Decimal,
    pub slippage_and_fx_charges: Decimal,
    pub currency: Currency,
    pub disposal_fx: Option<TaxFxRate>,
    pub selected_lot_ids: Vec<String>,
    pub withholding: Option<TaxWithholding>,
    pub withholding_confirmed_absent: bool,
    pub withholding_unavailable_reason: Option<String>,
    /// If supplied, this must reconcile exactly to calculated gross/net sale cash.
    pub broker_cash: Option<TaxCashObservation>,
    /// Scenario value of unsold shares in tax currency, preserving partial-sale value.
    pub remaining_position_value_tax_currency: Decimal,
    pub assumptions_reference: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaxReplacementScenario {
    pub disposal: TaxDisposalScenario,
    pub replacement_acquisition_cost_tax_currency: Decimal,
    /// Unrealized mark at comparison_horizon; a subsequent disposal is not modeled.
    pub replacement_terminal_value_tax_currency: Decimal,
    pub replacement_costs_tax_currency: Decimal,
    pub assumptions_reference: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaxSafetySnapshot {
    pub exit_latched: bool,
    pub protective_exit_active: bool,
    pub entry_slot_available: bool,
    pub replacement_path_approved: bool,
    pub replacement_risk_valid: bool,
    pub original_stop: Decimal,
    pub original_target: Decimal,
    pub confidence_gate: u8,
    pub leverage_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaxInput {
    pub instrument_id: InstrumentId,
    pub assessment_tax_date: NaiveDate,
    pub comparison_tax_date: NaiveDate,
    pub context: Option<TaxContext>,
    pub context_unavailable_reason: Option<String>,
    pub rules: Option<EffectiveRuleSet>,
    pub rules_unavailable_reason: Option<String>,
    pub lots: Option<Vec<CostBasisLot>>,
    pub lots_unavailable_reason: Option<String>,
    pub splits: Vec<TaxSplit>,
    pub annual_state: Option<AnnualTaxState>,
    pub annual_state_unavailable_reason: Option<String>,
    /// Owner-supplied no-realization portfolio value in tax currency at the same horizon.
    pub no_action_value_tax_currency: Option<Decimal>,
    pub no_action_value_unavailable_reason: Option<String>,
    pub comparison_horizon: DateTime<Utc>,
    pub assumptions_reference: String,
    pub hold_to_target: Option<TaxDisposalScenario>,
    pub hold_to_target_unavailable_reason: Option<String>,
    pub sell_replace: Option<TaxReplacementScenario>,
    pub sell_replace_unavailable_reason: Option<String>,
    pub safety: TaxSafetySnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaxAssessmentStatus {
    Illustrative,
    Partial,
    EstimateUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaxScenarioKind {
    NoAction,
    HoldToTarget,
    SellReplace,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaxAmountRange {
    pub lower: Decimal,
    pub central: Decimal,
    pub upper: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaxLotAllocation {
    pub lot_id: String,
    pub sold_quantity: Decimal,
    pub remaining_quantity: Decimal,
    pub consumed_basis_tax_currency: Decimal,
    pub remaining_basis_tax_currency: Decimal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaxScenarioEstimate {
    pub kind: TaxScenarioKind,
    pub pre_tax_value_tax_currency: Decimal,
    pub immediate_sale_cash_tax_currency: Decimal,
    pub realized_gain_tax_currency: Decimal,
    pub withholding_tax_currency: Decimal,
    pub estimated_annual_liability: TaxAmountRange,
    pub incremental_final_liability: TaxAmountRange,
    pub incremental_annual_settlement: TaxAmountRange,
    pub after_tax_value_tax_currency: TaxAmountRange,
    pub lot_allocations: Vec<TaxLotAllocation>,
    pub assumptions_reference: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaxAssumptionSnapshot {
    pub rule_version: String,
    pub rule_source_reference: String,
    pub rule_effective_from: DateTime<Utc>,
    pub rule_effective_until: DateTime<Utc>,
    pub rule_source_published_at: DateTime<Utc>,
    pub rule_owner_reviewed_at: DateTime<Utc>,
    pub assessment_tax_date: NaiveDate,
    pub comparison_tax_date: NaiveDate,
    pub tax_currency: Currency,
    pub gains_rate: TaxRateRange,
    pub dividend_rate: TaxRateRange,
    pub lot_method: TaxLotMethod,
    pub loss_treatment: TaxLossTreatment,
    pub allowance_allocation: TaxAllowanceAllocation,
    pub confirmed_allowance: Decimal,
    pub confirmed_loss_carryforward: Decimal,
    pub baseline_gross_stock_gains: Decimal,
    pub baseline_stock_losses: Decimal,
    pub baseline_gross_dividends: Decimal,
    pub carryforward_permitted: bool,
    pub acquisition_fees_in_basis: bool,
    pub sale_costs_deductible: bool,
    pub withholding_credit_permitted: bool,
    pub excess_withholding_refundable: bool,
    pub acquisition_disposal_date_fx_approved: bool,
    pub fx_observations_and_assumptions: Vec<TaxFxRate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaxAssessment {
    pub assessed_at: DateTime<Utc>,
    pub status: TaxAssessmentStatus,
    pub assumptions: Option<TaxAssumptionSnapshot>,
    pub no_action: Option<TaxScenarioEstimate>,
    pub hold_to_target: Option<TaxScenarioEstimate>,
    pub sell_replace: Option<TaxScenarioEstimate>,
    /// Exact copy: an illustration cannot alter exits, targets or confidence gates.
    pub preserved_safety: TaxSafetySnapshot,
    pub unavailable_reasons: Vec<String>,
    pub limitations: Vec<String>,
}

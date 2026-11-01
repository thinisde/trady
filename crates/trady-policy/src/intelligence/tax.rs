//! Checked-decimal illustrations using owner-approved rules and confirmed lots.
//! Pure reporting: no executor, order types, model calls, I/O or policy mutation.

use chrono::{DateTime, Datelike, NaiveDate, Utc};
use rust_decimal::Decimal;
use std::collections::{BTreeMap, BTreeSet};
use trady_domain::intelligence::tax::*;
use trady_domain::{Currency, Error, Result, require};

use crate::{add, div, mul, sub};

fn present(value: &str) -> bool {
    !value.trim().is_empty()
}

fn reason(value: &Option<String>, label: &str, output: &mut Vec<String>) -> Result<()> {
    require(
        value.as_deref().is_some_and(present),
        "missing tax input must have an explicit reason",
    )?;
    // Do not echo owner-provided prose, which may contain unnecessary identity data.
    output.push(label.into());
    Ok(())
}

fn rate_valid(rate: &TaxRateRange) -> bool {
    rate.lower >= Decimal::ZERO
        && rate.lower <= rate.central
        && rate.central <= rate.upper
        && rate.upper <= Decimal::ONE
}

fn range(lower: Decimal, central: Decimal, upper: Decimal) -> TaxAmountRange {
    TaxAmountRange {
        lower: lower.min(central).min(upper),
        central,
        upper: lower.max(central).max(upper),
    }
}

fn zero_range() -> TaxAmountRange {
    range(Decimal::ZERO, Decimal::ZERO, Decimal::ZERO)
}

// Conservative interval arithmetic keeps uncertainty rather than comparing only point rates.
fn difference(a: &TaxAmountRange, b: &TaxAmountRange) -> Result<TaxAmountRange> {
    Ok(range(
        sub(a.lower, b.upper)?,
        sub(a.central, b.central)?,
        sub(a.upper, b.lower)?,
    ))
}

fn fx_rate(
    from: &Currency,
    to: &Currency,
    fx: Option<&TaxFxRate>,
    date: NaiveDate,
    at: DateTime<Utc>,
    assessment_tax_date: NaiveDate,
    illustrative: bool,
) -> Result<Decimal> {
    if from == to && fx.is_none() {
        return Ok(Decimal::ONE);
    }
    let fx = fx.ok_or_else(|| Error("documented valuation-date tax FX is required".into()))?;
    require(
        &fx.from == from
            && &fx.to == to
            && fx.rate > Decimal::ZERO
            && fx.valuation_date == date
            && fx.source_published_at <= fx.available_at
            && fx.available_at <= at
            && fx.owner_reviewed
            && present(&fx.reference)
            && (!fx.illustrative_assumption || illustrative)
            && (date <= assessment_tax_date || fx.illustrative_assumption)
            && (from != to || fx.rate == Decimal::ONE),
        "unreviewed, substituted or wrong-date/currency tax FX",
    )?;
    Ok(fx.rate)
}

#[derive(Clone)]
struct Lot {
    id: String,
    acquired_at: DateTime<Utc>,
    quantity: Decimal,
    basis: Decimal,
}

fn prepare_lots(
    input: &TaxInput,
    context: &TaxContext,
    rules: &EffectiveRuleSet,
    at: DateTime<Utc>,
) -> Result<Vec<Lot>> {
    let mut seen = BTreeSet::new();
    let mut lots = Vec::new();
    for lot in input
        .lots
        .as_ref()
        .expect("required lots checked by assess")
    {
        require(
            present(&lot.lot_id)
                && seen.insert(&lot.lot_id)
                && lot.instrument_id == input.instrument_id
                && lot.quantity > Decimal::ZERO
                && lot.unit_cost >= Decimal::ZERO
                && lot.acquisition_fees >= Decimal::ZERO
                && lot.acquired_at <= lot.available_at
                && lot.tax_valuation_date <= input.assessment_tax_date
                && lot.available_at <= at
                && lot.broker_confirmed
                && present(&lot.statement_reference),
            "unconfirmed, duplicate or invalid actual acquisition lot",
        )?;
        let fx = fx_rate(
            &lot.currency,
            &context.tax_currency,
            lot.acquisition_fx.as_ref(),
            lot.tax_valuation_date,
            at,
            input.assessment_tax_date,
            false,
        )?;
        let acquisition_cost = mul(lot.quantity, lot.unit_cost)?;
        let basis = mul(
            if rules.acquisition_fees_in_basis {
                add(acquisition_cost, lot.acquisition_fees)?
            } else {
                acquisition_cost
            },
            fx,
        )?;
        lots.push(Lot {
            id: lot.lot_id.clone(),
            acquired_at: lot.acquired_at,
            quantity: lot.quantity,
            basis,
        });
    }
    let mut splits: Vec<&TaxSplit> = input.splits.iter().collect();
    splits.sort_by_key(|s| (s.effective_at, s.action_id.as_str()));
    let mut actions = BTreeMap::new();
    for split in splits {
        require(
            present(&split.action_id)
                && split.instrument_id == input.instrument_id
                && split.numerator > Decimal::ZERO
                && split.denominator > Decimal::ZERO
                && split.effective_at <= split.available_at
                && split.available_at <= at
                && split.broker_confirmed
                && present(&split.reference),
            "unconfirmed/invalid corporate action",
        )?;
        if let Some(previous) = actions.insert(split.action_id.as_str(), split) {
            require(previous == split, "conflicting duplicate split")?;
            continue;
        }
        for lot in &mut lots {
            if lot.acquired_at < split.effective_at {
                let numerator = mul(lot.quantity, split.numerator)?;
                let quantity = div(numerator, split.denominator)?;
                require(
                    quantity > Decimal::ZERO && mul(quantity, split.denominator)? == numerator,
                    "split quantity cannot be represented exactly; reconcile cash-in-lieu",
                )?;
                lot.quantity = quantity;
                // A split changes quantity, never total tax basis.
            }
        }
    }
    lots.sort_by_key(|lot| (lot.acquired_at, lot.id.clone()));
    Ok(lots)
}

fn allocate(
    lots: &[Lot],
    sale: &TaxDisposalScenario,
    rules: &EffectiveRuleSet,
) -> Result<Vec<TaxLotAllocation>> {
    let order: Vec<&Lot> = match rules.lot_method {
        TaxLotMethod::Fifo => {
            require(
                sale.selected_lot_ids.is_empty(),
                "FIFO cannot select favorable lots",
            )?;
            lots.iter().collect()
        }
        TaxLotMethod::SpecificIdentification => {
            require(
                rules.specific_identification_permitted && !sale.selected_lot_ids.is_empty(),
                "specific identification is not explicitly permitted",
            )?;
            let mut selected = BTreeSet::new();
            let mut ordered = Vec::new();
            for id in &sale.selected_lot_ids {
                require(selected.insert(id), "duplicate selected lot")?;
                ordered.push(
                    lots.iter()
                        .find(|lot| &lot.id == id)
                        .ok_or_else(|| Error("selected lot is unknown".into()))?,
                );
            }
            ordered.extend(lots.iter().filter(|lot| !selected.contains(&lot.id)));
            ordered
        }
    };
    let mut remaining = sale.quantity;
    let mut allocations = Vec::new();
    for lot in order {
        require(
            lot.acquired_at <= sale.disposed_at,
            "sale predates acquisition lot",
        )?;
        let eligible =
            rules.lot_method == TaxLotMethod::Fifo || sale.selected_lot_ids.contains(&lot.id);
        let quantity = if eligible {
            lot.quantity.min(remaining)
        } else {
            Decimal::ZERO
        };
        let consumed = if quantity == lot.quantity {
            lot.basis
        } else {
            div(mul(lot.basis, quantity)?, lot.quantity)?
        };
        let remaining_basis = sub(lot.basis, consumed)?;
        require(
            consumed >= Decimal::ZERO
                && remaining_basis >= Decimal::ZERO
                && add(consumed, remaining_basis)? == lot.basis,
            "partial sale basis conservation failed",
        )?;
        allocations.push(TaxLotAllocation {
            lot_id: lot.id.clone(),
            sold_quantity: quantity,
            remaining_quantity: sub(lot.quantity, quantity)?,
            consumed_basis_tax_currency: consumed,
            remaining_basis_tax_currency: remaining_basis,
        });
        remaining = sub(remaining, quantity)?;
    }
    require(
        remaining == Decimal::ZERO,
        "sale exceeds actual available/selected lots",
    )?;
    Ok(allocations)
}

fn validate_withholding(
    value: &TaxWithholding,
    at: DateTime<Utc>,
    illustrative: bool,
) -> Result<()> {
    require(
        present(&value.event_id)
            && present(&value.reference)
            && value.amount_tax_currency >= Decimal::ZERO
            && value.available_at <= at
            && (value.broker_confirmed || (illustrative && value.illustrative_assumption))
            && (illustrative || !value.illustrative_assumption)
            && (value.illustrative_assumption || value.paid_at <= value.available_at),
        "unconfirmed/invalid withholding deduction",
    )
}

fn same_withholding(a: &TaxWithholding, b: &TaxWithholding) -> bool {
    // Two statements may have different document references for the same payment.
    a.event_id == b.event_id
        && a.amount_tax_currency == b.amount_tax_currency
        && a.related_disposal_id == b.related_disposal_id
        && a.paid_at == b.paid_at
        && a.broker_confirmed == b.broker_confirmed
        && a.illustrative_assumption == b.illustrative_assumption
}

fn annual_withholding(
    state: &AnnualTaxState,
    excluded: &BTreeSet<&str>,
    at: DateTime<Utc>,
) -> Result<BTreeMap<String, TaxWithholding>> {
    let mut seen = BTreeMap::new();
    let mut payments = BTreeMap::new();
    for value in &state.withholding {
        validate_withholding(value, at, false)?;
        if let Some(previous) = seen.insert(value.event_id.clone(), value.clone()) {
            require(
                same_withholding(&previous, value),
                "conflicting repeated withholding event",
            )?;
        }
        // Counterfactual baseline excludes the proposed disposal and its payment.
        if !value
            .related_disposal_id
            .as_deref()
            .is_some_and(|id| excluded.contains(id))
        {
            payments.insert(value.event_id.clone(), value.clone());
        }
    }
    Ok(payments)
}

fn liability(
    state: &AnnualTaxState,
    rules: &EffectiveRuleSet,
    gain: Decimal,
) -> Result<TaxAmountRange> {
    let mut gains = state.gross_stock_gains.expect("confirmed state checked");
    let mut losses = state.stock_losses.expect("confirmed state checked");
    let mut dividends = state.gross_dividends.expect("confirmed state checked");
    if gain >= Decimal::ZERO {
        gains = add(gains, gain)?;
    } else {
        losses = sub(losses, gain)?;
    }
    let carry = state
        .confirmed_loss_carryforward
        .expect("confirmed state checked");
    if rules.loss_treatment != TaxLossTreatment::NoOffsets {
        let loss_budget = add(losses, carry)?;
        let offset = gains.min(loss_budget);
        gains = sub(gains, offset)?;
        if rules.loss_treatment == TaxLossTreatment::GainsThenDividends {
            dividends = sub(dividends, sub(loss_budget, offset)?.min(dividends))?;
        }
    }
    let allowance = state.confirmed_allowance.expect("confirmed state checked");
    match rules.allowance_allocation {
        TaxAllowanceAllocation::GainsOnly => {
            gains = sub(gains, gains.min(allowance))?;
        }
        TaxAllowanceAllocation::GainsThenDividends => {
            let used = gains.min(allowance);
            gains = sub(gains, used)?;
            dividends = sub(dividends, dividends.min(sub(allowance, used)?))?;
        }
        TaxAllowanceAllocation::DividendsThenGains => {
            let used = dividends.min(allowance);
            dividends = sub(dividends, used)?;
            gains = sub(gains, gains.min(sub(allowance, used)?))?;
        }
    }
    Ok(range(
        add(
            mul(gains, rules.gains_rate.lower)?,
            mul(dividends, rules.dividend_rate.lower)?,
        )?,
        add(
            mul(gains, rules.gains_rate.central)?,
            mul(dividends, rules.dividend_rate.central)?,
        )?,
        add(
            mul(gains, rules.gains_rate.upper)?,
            mul(dividends, rules.dividend_rate.upper)?,
        )?,
    ))
}

fn settlement(
    liability: &TaxAmountRange,
    credits: Decimal,
    refundable: bool,
) -> Result<TaxAmountRange> {
    let apply = |tax| -> Result<Decimal> {
        let balance = sub(tax, credits)?;
        Ok(if refundable {
            balance
        } else {
            balance.max(Decimal::ZERO)
        })
    };
    Ok(range(
        apply(liability.lower)?,
        apply(liability.central)?,
        apply(liability.upper)?,
    ))
}

fn credit_sum(
    payments: &BTreeMap<String, TaxWithholding>,
    rules: &EffectiveRuleSet,
) -> Result<Decimal> {
    if !rules.withholding_credit_permitted {
        return Ok(Decimal::ZERO);
    }
    payments
        .values()
        .try_fold(Decimal::ZERO, |sum, v| add(sum, v.amount_tax_currency))
}

struct ScenarioContext<'a> {
    input: &'a TaxInput,
    context: &'a TaxContext,
    rules: &'a EffectiveRuleSet,
    annual: &'a AnnualTaxState,
    lots: &'a [Lot],
    baseline_liability: &'a TaxAmountRange,
    baseline_settlement: &'a TaxAmountRange,
    baseline_payments: &'a BTreeMap<String, TaxWithholding>,
    at: DateTime<Utc>,
}

fn sale_estimate(
    sale: &TaxDisposalScenario,
    kind: TaxScenarioKind,
    cx: &ScenarioContext<'_>,
) -> Result<TaxScenarioEstimate> {
    require(
        present(&sale.disposal_id)
            && present(&sale.assumptions_reference)
            && sale.quantity > Decimal::ZERO
            && sale.unit_proceeds >= Decimal::ZERO
            && sale.sale_fees >= Decimal::ZERO
            && sale.slippage_and_fx_charges >= Decimal::ZERO
            && sale.remaining_position_value_tax_currency >= Decimal::ZERO
            && sale.disposed_at <= cx.input.comparison_horizon
            && sale.disposed_at >= cx.rules.effective_from
            && sale.disposed_at < cx.rules.effective_until
            && sale.tax_valuation_date.year() == cx.context.tax_year
            && sale.tax_valuation_date <= cx.input.comparison_tax_date,
        "invalid or out-of-rule-period tax sale scenario",
    )?;
    let fx = fx_rate(
        &sale.currency,
        &cx.context.tax_currency,
        sale.disposal_fx.as_ref(),
        sale.tax_valuation_date,
        cx.at,
        cx.input.assessment_tax_date,
        true,
    )?;
    let gross = mul(mul(sale.quantity, sale.unit_proceeds)?, fx)?;
    let fees = mul(add(sale.sale_fees, sale.slippage_and_fx_charges)?, fx)?;
    let before_withholding = sub(gross, fees)?;
    require(
        before_withholding >= Decimal::ZERO,
        "sale charges exceed proceeds",
    )?;
    let allocations = allocate(cx.lots, sale, cx.rules)?;
    let basis = allocations.iter().try_fold(Decimal::ZERO, |sum, a| {
        add(sum, a.consumed_basis_tax_currency)
    })?;
    let remaining_quantity = allocations
        .iter()
        .try_fold(Decimal::ZERO, |sum, a| add(sum, a.remaining_quantity))?;
    require(
        remaining_quantity > Decimal::ZERO
            || sale.remaining_position_value_tax_currency == Decimal::ZERO,
        "full sale cannot retain a position value",
    )?;
    let gain = sub(
        if cx.rules.sale_costs_deductible {
            before_withholding
        } else {
            gross
        },
        basis,
    )?;
    let mut payments = cx.baseline_payments.clone();
    let withholding = if let Some(value) = &sale.withholding {
        require(
            !sale.withholding_confirmed_absent
                && value.related_disposal_id.as_deref() == Some(sale.disposal_id.as_str()),
            "withholding does not match scenario disposal",
        )?;
        validate_withholding(value, cx.at, true)?;
        for existing in &cx.annual.withholding {
            if existing.event_id == value.event_id {
                require(
                    same_withholding(existing, value),
                    "cash/annual withholding mismatch",
                )?;
            }
        }
        if let Some(previous) = payments.insert(value.event_id.clone(), value.clone()) {
            require(
                same_withholding(&previous, value),
                "conflicting withholding payment",
            )?;
        }
        value.amount_tax_currency
    } else {
        require(
            sale.withholding_confirmed_absent,
            "unknown sale withholding is not zero",
        )?;
        require(
            !cx.annual.withholding.iter().any(|v| {
                v.related_disposal_id.as_deref() == Some(sale.disposal_id.as_str())
                    && v.amount_tax_currency > Decimal::ZERO
            }),
            "withholding absence contradicts broker ledger",
        )?;
        Decimal::ZERO
    };
    let immediate_cash = sub(before_withholding, withholding)?;
    require(
        immediate_cash >= Decimal::ZERO,
        "withholding exceeds sale cash",
    )?;
    if let Some(cash) = &sale.broker_cash {
        let expected = if cash.basis == TaxCashBasis::NetAfterWithholding {
            immediate_cash
        } else {
            before_withholding
        };
        require(
            cash.amount_tax_currency == expected
                && cash.available_at <= cx.at
                && present(&cash.reference),
            "broker cash does not reconcile to gross/net withholding once",
        )?;
    }
    let annual_liability = liability(cx.annual, cx.rules, gain)?;
    let annual_settlement = settlement(
        &annual_liability,
        credit_sum(&payments, cx.rules)?,
        cx.rules.excess_withholding_refundable,
    )?;
    let incremental_settlement = difference(&annual_settlement, cx.baseline_settlement)?;
    let pre_tax = add(
        before_withholding,
        sale.remaining_position_value_tax_currency,
    )?;
    let cash_and_remaining = add(immediate_cash, sale.remaining_position_value_tax_currency)?;
    let after_tax = range(
        sub(cash_and_remaining, incremental_settlement.upper)?,
        sub(cash_and_remaining, incremental_settlement.central)?,
        sub(cash_and_remaining, incremental_settlement.lower)?,
    );
    Ok(TaxScenarioEstimate {
        kind,
        pre_tax_value_tax_currency: pre_tax,
        immediate_sale_cash_tax_currency: immediate_cash,
        realized_gain_tax_currency: gain,
        withholding_tax_currency: withholding,
        estimated_annual_liability: annual_liability,
        incremental_final_liability: difference(
            &liability(cx.annual, cx.rules, gain)?,
            cx.baseline_liability,
        )?,
        incremental_annual_settlement: incremental_settlement,
        after_tax_value_tax_currency: after_tax,
        lot_allocations: allocations,
        assumptions_reference: sale.assumptions_reference.clone(),
    })
}

pub fn assess(input: &TaxInput, at: DateTime<Utc>) -> Result<TaxAssessment> {
    let mut result = TaxAssessment {
        assessed_at: at, status: TaxAssessmentStatus::EstimateUnavailable, assumptions: None,
        no_action: None, hold_to_target: None, sell_replace: None, preserved_safety: input.safety.clone(),
        unavailable_reasons: Vec::new(), limitations: vec![
            "owner-configured linear tax illustrations; no legal inference, tax advice or order authority".into(),
            "after-tax ranges conservatively propagate rate uncertainty; no timing/deferral value is invented".into(),
            "replacement terminal values are supplied scenarios, not forecasts or expected-return signals".into(),
            "replacement terminal value is an unrealized mark; subsequent replacement-sale tax is not modeled".into(),
        ],
    };
    require(
        input.comparison_horizon >= at
            && input.assessment_tax_date <= input.comparison_tax_date
            && present(&input.assumptions_reference)
            && input.safety.original_stop > Decimal::ZERO
            && input.safety.original_target > Decimal::ZERO
            && input.safety.confidence_gate <= 100,
        "invalid tax comparison/safety snapshot",
    )?;
    if input.context.is_none() {
        reason(
            &input.context_unavailable_reason,
            "missing approved context",
            &mut result.unavailable_reasons,
        )?;
    }
    if input.rules.is_none() {
        reason(
            &input.rules_unavailable_reason,
            "missing approved rules",
            &mut result.unavailable_reasons,
        )?;
    }
    if input.lots.is_none() {
        reason(
            &input.lots_unavailable_reason,
            "missing actual acquisition lots",
            &mut result.unavailable_reasons,
        )?;
    }
    if input.annual_state.is_none() {
        reason(
            &input.annual_state_unavailable_reason,
            "missing annual state",
            &mut result.unavailable_reasons,
        )?;
    }
    if input.no_action_value_tax_currency.is_none() {
        reason(
            &input.no_action_value_unavailable_reason,
            "missing no-action valuation",
            &mut result.unavailable_reasons,
        )?;
    }
    if !result.unavailable_reasons.is_empty() {
        return Ok(result);
    }
    let context = input.context.as_ref().expect("checked context");
    let rules = input.rules.as_ref().expect("checked rules");
    let annual = input.annual_state.as_ref().expect("checked annual state");
    require(
        rate_valid(&rules.gains_rate) && rate_valid(&rules.dividend_rate),
        "invalid supplied tax-rate interval",
    )?;
    let context_valid = present(&context.jurisdiction)
        && present(&context.account_wrapper)
        && present(&context.broker_entity)
        && context.residence_owner_confirmed
        && context.account_treatment_owner_confirmed
        && context.statement_as_of <= context.confirmed_at
        && context.confirmed_at <= at
        && context.valid_until > input.comparison_horizon
        && present(&context.statement_reference)
        && context.tax_year == input.assessment_tax_date.year()
        && input.comparison_tax_date.year() == context.tax_year;
    let rules_valid = rules.owner_approved
        && rules.linear_scenario_supported
        && present(&rules.version)
        && rules.jurisdiction == context.jurisdiction
        && rules.tax_year == context.tax_year
        && rules.account_wrapper == context.account_wrapper
        && rules.broker_entity == context.broker_entity
        && rules.tax_currency == context.tax_currency
        && rules.effective_from <= at
        && input.comparison_horizon < rules.effective_until
        && rules.source_published_at <= rules.available_at
        && rules.available_at <= rules.owner_reviewed_at
        && rules.owner_reviewed_at <= at
        && present(&rules.source_reference)
        && rules.acquisition_disposal_date_fx_approved
        && (rules.lot_method != TaxLotMethod::SpecificIdentification
            || rules.specific_identification_permitted);
    if !context_valid {
        result.unavailable_reasons.push(
            "unconfirmed/expired/incomplete jurisdiction, residence or account treatment".into(),
        );
    }
    if !rules_valid {
        result
            .unavailable_reasons
            .push("unapproved/mismatched/unsupported or out-of-date prospective tax rules".into());
    }
    if input
        .lots
        .as_ref()
        .is_none_or(|lots| lots.is_empty() || lots.iter().any(|lot| !lot.broker_confirmed))
    {
        result
            .unavailable_reasons
            .push("actual confirmed lots unavailable; no invented basis".into());
    }
    if [
        annual.gross_stock_gains,
        annual.stock_losses,
        annual.gross_dividends,
        annual.confirmed_allowance,
        annual.confirmed_loss_carryforward,
    ]
    .iter()
    .any(Option::is_none)
    {
        reason(
            &annual.unknown_reason,
            "unknown allowance, carryforward or reconciled income",
            &mut result.unavailable_reasons,
        )?;
    }
    if !annual.owner_confirmed || annual.available_at > at || !present(&annual.statement_reference)
    {
        result
            .unavailable_reasons
            .push("annual account state is not confirmed/available".into());
    }
    if !result.unavailable_reasons.is_empty() {
        return Ok(result);
    }
    require(
        [
            annual.gross_stock_gains,
            annual.stock_losses,
            annual.gross_dividends,
            annual.confirmed_allowance,
            annual.confirmed_loss_carryforward,
        ]
        .iter()
        .all(|v| v.is_some_and(|x| x >= Decimal::ZERO)),
        "negative annual gain/loss/allowance aggregate",
    )?;
    if annual
        .confirmed_loss_carryforward
        .is_some_and(|v| v > Decimal::ZERO)
        && (!rules.carryforward_permitted || rules.loss_treatment == TaxLossTreatment::NoOffsets)
    {
        result
            .unavailable_reasons
            .push("documented carryforward treatment unsupported by supplied rules".into());
        return Ok(result);
    }
    let no_action_value = input
        .no_action_value_tax_currency
        .expect("checked no-action value");
    require(
        no_action_value >= Decimal::ZERO,
        "negative no-action position value",
    )?;
    let lots = prepare_lots(input, context, rules, at)?;
    let mut excluded = BTreeSet::new();
    if let Some(sale) = &input.hold_to_target {
        require(
            excluded.insert(sale.disposal_id.as_str()),
            "duplicate proposed tax disposal",
        )?;
    }
    if let Some(replacement) = &input.sell_replace {
        require(
            excluded.insert(replacement.disposal.disposal_id.as_str()),
            "duplicate proposed tax disposal",
        )?;
    }
    let baseline_payments = annual_withholding(annual, &excluded, at)?;
    let baseline_liability = liability(annual, rules, Decimal::ZERO)?;
    let baseline_settlement = settlement(
        &baseline_liability,
        credit_sum(&baseline_payments, rules)?,
        rules.excess_withholding_refundable,
    )?;
    result.assumptions = Some(TaxAssumptionSnapshot {
        rule_version: rules.version.clone(),
        rule_source_reference: rules.source_reference.clone(),
        rule_effective_from: rules.effective_from,
        rule_effective_until: rules.effective_until,
        rule_source_published_at: rules.source_published_at,
        rule_owner_reviewed_at: rules.owner_reviewed_at,
        assessment_tax_date: input.assessment_tax_date,
        comparison_tax_date: input.comparison_tax_date,
        tax_currency: context.tax_currency.clone(),
        gains_rate: rules.gains_rate.clone(),
        dividend_rate: rules.dividend_rate.clone(),
        lot_method: rules.lot_method,
        loss_treatment: rules.loss_treatment,
        allowance_allocation: rules.allowance_allocation,
        confirmed_allowance: annual.confirmed_allowance.expect("checked allowance"),
        confirmed_loss_carryforward: annual
            .confirmed_loss_carryforward
            .expect("checked carryforward"),
        baseline_gross_stock_gains: annual.gross_stock_gains.expect("checked gains"),
        baseline_stock_losses: annual.stock_losses.expect("checked losses"),
        baseline_gross_dividends: annual.gross_dividends.expect("checked dividends"),
        carryforward_permitted: rules.carryforward_permitted,
        acquisition_fees_in_basis: rules.acquisition_fees_in_basis,
        sale_costs_deductible: rules.sale_costs_deductible,
        withholding_credit_permitted: rules.withholding_credit_permitted,
        excess_withholding_refundable: rules.excess_withholding_refundable,
        acquisition_disposal_date_fx_approved: rules.acquisition_disposal_date_fx_approved,
        fx_observations_and_assumptions: input
            .lots
            .as_ref()
            .expect("checked lots")
            .iter()
            .filter_map(|lot| lot.acquisition_fx.clone())
            .collect(),
    });
    result.no_action = Some(TaxScenarioEstimate {
        kind: TaxScenarioKind::NoAction,
        pre_tax_value_tax_currency: no_action_value,
        immediate_sale_cash_tax_currency: Decimal::ZERO,
        realized_gain_tax_currency: Decimal::ZERO,
        withholding_tax_currency: Decimal::ZERO,
        estimated_annual_liability: baseline_liability.clone(),
        incremental_final_liability: zero_range(),
        incremental_annual_settlement: zero_range(),
        after_tax_value_tax_currency: range(no_action_value, no_action_value, no_action_value),
        lot_allocations: lots
            .iter()
            .map(|lot| TaxLotAllocation {
                lot_id: lot.id.clone(),
                sold_quantity: Decimal::ZERO,
                remaining_quantity: lot.quantity,
                consumed_basis_tax_currency: Decimal::ZERO,
                remaining_basis_tax_currency: lot.basis,
            })
            .collect(),
        assumptions_reference: input.assumptions_reference.clone(),
    });
    let cx = ScenarioContext {
        input,
        context,
        rules,
        annual,
        lots: &lots,
        baseline_liability: &baseline_liability,
        baseline_settlement: &baseline_settlement,
        baseline_payments: &baseline_payments,
        at,
    };
    if let Some(sale) = &input.hold_to_target {
        if sale.withholding.is_none() && !sale.withholding_confirmed_absent {
            reason(
                &sale.withholding_unavailable_reason,
                "target withholding unknown",
                &mut result.unavailable_reasons,
            )?;
        } else {
            result.hold_to_target = Some(sale_estimate(sale, TaxScenarioKind::HoldToTarget, &cx)?);
            if let Some(fx) = &sale.disposal_fx {
                result
                    .assumptions
                    .as_mut()
                    .expect("approved assumptions")
                    .fx_observations_and_assumptions
                    .push(fx.clone());
            }
        }
    } else {
        reason(
            &input.hold_to_target_unavailable_reason,
            "hold-to-target scenario unavailable",
            &mut result.unavailable_reasons,
        )?;
    }
    let safety = &input.safety;
    if let Some(replacement) = &input.sell_replace {
        if safety.exit_latched
            || safety.protective_exit_active
            || !safety.entry_slot_available
            || !safety.replacement_path_approved
            || !safety.replacement_risk_valid
            || safety.leverage_enabled
        {
            result.unavailable_reasons.push("replacement illustration blocked by existing exit/entry/risk constraints; exits unchanged".into());
        } else if replacement.disposal.withholding.is_none()
            && !replacement.disposal.withholding_confirmed_absent
        {
            reason(
                &replacement.disposal.withholding_unavailable_reason,
                "replacement withholding unknown",
                &mut result.unavailable_reasons,
            )?;
        } else {
            require(
                replacement.replacement_acquisition_cost_tax_currency >= Decimal::ZERO
                    && replacement.replacement_terminal_value_tax_currency >= Decimal::ZERO
                    && replacement.replacement_costs_tax_currency >= Decimal::ZERO
                    && present(&replacement.assumptions_reference)
                    && replacement.disposal.disposed_at < input.comparison_horizon,
                "invalid replacement scenario values/horizon",
            )?;
            let mut estimate =
                sale_estimate(&replacement.disposal, TaxScenarioKind::SellReplace, &cx)?;
            let cost = add(
                replacement.replacement_acquisition_cost_tax_currency,
                replacement.replacement_costs_tax_currency,
            )?;
            require(
                cost <= estimate.immediate_sale_cash_tax_currency,
                "replacement requires unavailable cash/leverage",
            )?;
            let difference = sub(replacement.replacement_terminal_value_tax_currency, cost)?;
            estimate.pre_tax_value_tax_currency =
                add(estimate.pre_tax_value_tax_currency, difference)?;
            estimate.after_tax_value_tax_currency = range(
                add(estimate.after_tax_value_tax_currency.lower, difference)?,
                add(estimate.after_tax_value_tax_currency.central, difference)?,
                add(estimate.after_tax_value_tax_currency.upper, difference)?,
            );
            estimate.assumptions_reference = replacement.assumptions_reference.clone();
            result.sell_replace = Some(estimate);
            if let Some(fx) = &replacement.disposal.disposal_fx {
                result
                    .assumptions
                    .as_mut()
                    .expect("approved assumptions")
                    .fx_observations_and_assumptions
                    .push(fx.clone());
            }
        }
    } else {
        reason(
            &input.sell_replace_unavailable_reason,
            "sell/replace scenario unavailable",
            &mut result.unavailable_reasons,
        )?;
    }
    result.status = if result.unavailable_reasons.is_empty() {
        TaxAssessmentStatus::Illustrative
    } else {
        TaxAssessmentStatus::Partial
    };
    Ok(result)
}

#[cfg(test)]
mod tax_acceptance {
    use super::*;
    use chrono::Duration;
    use std::str::FromStr;
    use trady_domain::InstrumentId;

    fn d(value: &str) -> Decimal {
        Decimal::from_str(value).unwrap()
    }
    fn time(value: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value).unwrap().to_utc()
    }
    fn at() -> DateTime<Utc> {
        time("2026-10-31T14:00:00Z")
    }
    fn eur() -> Currency {
        Currency::new("EUR").unwrap()
    }
    fn rates(value: &str) -> TaxRateRange {
        TaxRateRange {
            lower: d(value),
            central: d(value),
            upper: d(value),
        }
    }
    fn sale(
        id: &str,
        quantity: &str,
        price: &str,
        fees: &str,
        remaining: &str,
    ) -> TaxDisposalScenario {
        TaxDisposalScenario {
            disposal_id: id.into(),
            disposed_at: at() + Duration::days(10),
            tax_valuation_date: (at() + Duration::days(10)).date_naive(),
            quantity: d(quantity),
            unit_proceeds: d(price),
            sale_fees: d(fees),
            slippage_and_fx_charges: Decimal::ZERO,
            currency: eur(),
            disposal_fx: None,
            selected_lot_ids: vec![],
            withholding: None,
            withholding_confirmed_absent: true,
            withholding_unavailable_reason: None,
            broker_cash: None,
            remaining_position_value_tax_currency: d(remaining),
            assumptions_reference: "synthetic-scenario".into(),
        }
    }
    fn fixture() -> TaxInput {
        let instrument = InstrumentId::new("stock").unwrap();
        TaxInput {
            instrument_id: instrument.clone(),
            assessment_tax_date: at().date_naive(),
            comparison_tax_date: (at() + Duration::days(10)).date_naive(),
            context: Some(TaxContext {
                jurisdiction: "SYNTHETIC_JURISDICTION".into(),
                tax_year: 2026,
                account_wrapper: "synthetic-wrapper".into(),
                broker_entity: "private-broker-entity".into(),
                tax_currency: eur(),
                residence_owner_confirmed: true,
                account_treatment_owner_confirmed: true,
                confirmed_at: at() - Duration::hours(1),
                statement_as_of: at() - Duration::days(1),
                statement_reference: "private-statement-reference".into(),
                valid_until: at() + Duration::days(30),
            }),
            context_unavailable_reason: None,
            rules: Some(EffectiveRuleSet {
                version: "synthetic-rules-v1".into(),
                jurisdiction: "SYNTHETIC_JURISDICTION".into(),
                tax_year: 2026,
                account_wrapper: "synthetic-wrapper".into(),
                broker_entity: "private-broker-entity".into(),
                tax_currency: eur(),
                effective_from: time("2026-01-01T00:00:00Z"),
                effective_until: time("2027-01-01T00:00:00Z"),
                source_published_at: time("2025-12-01T00:00:00Z"),
                available_at: time("2025-12-02T00:00:00Z"),
                owner_reviewed_at: time("2025-12-03T00:00:00Z"),
                source_reference: "synthetic-reviewed-rule".into(),
                owner_approved: true,
                linear_scenario_supported: true,
                lot_method: TaxLotMethod::Fifo,
                specific_identification_permitted: false,
                gains_rate: rates("0.25"),
                dividend_rate: rates("0.25"),
                loss_treatment: TaxLossTreatment::GainsThenDividends,
                allowance_allocation: TaxAllowanceAllocation::GainsThenDividends,
                carryforward_permitted: true,
                acquisition_fees_in_basis: true,
                sale_costs_deductible: true,
                withholding_credit_permitted: true,
                excess_withholding_refundable: true,
                acquisition_disposal_date_fx_approved: true,
            }),
            rules_unavailable_reason: None,
            lots: Some(vec![
                CostBasisLot {
                    lot_id: "old-lot".into(),
                    instrument_id: instrument.clone(),
                    acquired_at: time("2026-01-10T00:00:00Z"),
                    tax_valuation_date: time("2026-01-10T00:00:00Z").date_naive(),
                    quantity: d("2"),
                    unit_cost: d("40"),
                    acquisition_fees: d("2"),
                    currency: eur(),
                    acquisition_fx: None,
                    broker_confirmed: true,
                    available_at: time("2026-01-11T00:00:00Z"),
                    statement_reference: "opaque-lot-reference".into(),
                },
                CostBasisLot {
                    lot_id: "new-lot".into(),
                    instrument_id: instrument,
                    acquired_at: time("2026-07-10T00:00:00Z"),
                    tax_valuation_date: time("2026-07-10T00:00:00Z").date_naive(),
                    quantity: d("1"),
                    unit_cost: d("70"),
                    acquisition_fees: d("1"),
                    currency: eur(),
                    acquisition_fx: None,
                    broker_confirmed: true,
                    available_at: time("2026-07-11T00:00:00Z"),
                    statement_reference: "opaque-lot-reference-2".into(),
                },
            ]),
            lots_unavailable_reason: None,
            splits: vec![],
            annual_state: Some(AnnualTaxState {
                gross_stock_gains: Some(Decimal::ZERO),
                stock_losses: Some(Decimal::ZERO),
                gross_dividends: Some(Decimal::ZERO),
                confirmed_allowance: Some(Decimal::ZERO),
                confirmed_loss_carryforward: Some(Decimal::ZERO),
                unknown_reason: None,
                owner_confirmed: true,
                available_at: at() - Duration::minutes(1),
                statement_reference: "private-annual-statement".into(),
                withholding: vec![],
            }),
            annual_state_unavailable_reason: None,
            no_action_value_tax_currency: Some(d("270")),
            no_action_value_unavailable_reason: None,
            comparison_horizon: at() + Duration::days(10),
            assumptions_reference: "no-action-mark-assumption".into(),
            hold_to_target: Some(sale("target", "1", "100", "2", "180")),
            hold_to_target_unavailable_reason: None,
            sell_replace: Some(TaxReplacementScenario {
                disposal: {
                    let mut value = sale("replacement", "3", "90", "3", "0");
                    value.disposed_at = at();
                    value.tax_valuation_date = at().date_naive();
                    value
                },
                replacement_acquisition_cost_tax_currency: d("200"),
                replacement_terminal_value_tax_currency: d("240"),
                replacement_costs_tax_currency: d("2"),
                assumptions_reference: "synthetic-replacement-values".into(),
            }),
            sell_replace_unavailable_reason: None,
            safety: TaxSafetySnapshot {
                exit_latched: false,
                protective_exit_active: false,
                entry_slot_available: true,
                replacement_path_approved: true,
                replacement_risk_valid: true,
                original_stop: d("80"),
                original_target: d("100"),
                confidence_gate: 90,
                leverage_enabled: false,
            },
        }
    }

    #[test]
    fn no_action_target_and_replace_keep_pretax_cash_and_annual_liability_separate() {
        let result = assess(&fixture(), at()).unwrap();
        assert_eq!(result.status, TaxAssessmentStatus::Illustrative);
        let hold = result.no_action.unwrap();
        assert_eq!(hold.immediate_sale_cash_tax_currency, Decimal::ZERO);
        assert_eq!(hold.pre_tax_value_tax_currency, d("270"));
        assert_eq!(hold.incremental_final_liability, zero_range());
        let target = result.hold_to_target.unwrap();
        assert_eq!(target.pre_tax_value_tax_currency, d("278"));
        assert_eq!(target.immediate_sale_cash_tax_currency, d("98"));
        assert_eq!(target.realized_gain_tax_currency, d("57"));
        assert_eq!(target.incremental_final_liability.central, d("14.25"));
        assert_eq!(target.after_tax_value_tax_currency.central, d("263.75"));
        let replace = result.sell_replace.unwrap();
        assert_eq!(replace.pre_tax_value_tax_currency, d("305"));
        assert_eq!(replace.immediate_sale_cash_tax_currency, d("267"));
        assert_eq!(replace.realized_gain_tax_currency, d("114"));
        assert_eq!(replace.after_tax_value_tax_currency.central, d("276.5"));
    }

    #[test]
    fn missing_context_lots_residence_allowances_or_carryforward_never_means_zero_tax() {
        for mode in 0..9 {
            let mut input = fixture();
            match mode {
                0 => {
                    input.context = None;
                    input.context_unavailable_reason = Some("unknown residence".into());
                }
                1 => {
                    input.lots = None;
                    input.lots_unavailable_reason = Some("missing broker lots".into());
                }
                2 => {
                    input
                        .context
                        .as_mut()
                        .unwrap()
                        .account_treatment_owner_confirmed = false
                }
                3 => input.context.as_mut().unwrap().jurisdiction.clear(),
                4 => {
                    let state = input.annual_state.as_mut().unwrap();
                    state.confirmed_allowance = None;
                    state.unknown_reason = Some("exemption undocumented".into());
                }
                5 => {
                    let state = input.annual_state.as_mut().unwrap();
                    state.confirmed_loss_carryforward = None;
                    state.unknown_reason = Some("carryforward unknown".into());
                }
                6 => input.lots.as_mut().unwrap()[0].broker_confirmed = false,
                7 => input.lots = Some(vec![]),
                _ => {
                    input.rules = None;
                    input.rules_unavailable_reason = Some("rules unapproved".into());
                }
            }
            let result = assess(&input, at()).unwrap();
            assert_eq!(result.status, TaxAssessmentStatus::EstimateUnavailable);
            assert!(
                result.no_action.is_none()
                    && result.hold_to_target.is_none()
                    && result.sell_replace.is_none()
            );
            assert!(result.assumptions.is_none());
            assert!(!result.unavailable_reasons.is_empty());
            assert_eq!(result.preserved_safety, input.safety);
        }
    }

    #[test]
    fn fifo_partial_sale_includes_fees_and_conserves_basis() {
        let target = assess(&fixture(), at()).unwrap().hold_to_target.unwrap();
        assert_eq!(target.lot_allocations[0].lot_id, "old-lot");
        assert_eq!(
            target.lot_allocations[0].consumed_basis_tax_currency,
            d("41")
        );
        assert_eq!(
            target.lot_allocations[0].remaining_basis_tax_currency,
            d("41")
        );
        let total = target
            .lot_allocations
            .iter()
            .try_fold(Decimal::ZERO, |sum, lot| {
                add(
                    sum,
                    add(
                        lot.consumed_basis_tax_currency,
                        lot.remaining_basis_tax_currency,
                    )?,
                )
            })
            .unwrap();
        assert_eq!(total, d("153"));
        assert_eq!(target.lot_allocations[1].sold_quantity, Decimal::ZERO);
    }

    #[test]
    fn confirmed_split_and_fractional_partial_sales_preserve_quantity_and_total_basis() {
        let mut input = fixture();
        input.splits.push(TaxSplit {
            action_id: "split-2-for-1".into(),
            instrument_id: input.instrument_id.clone(),
            effective_at: time("2026-10-01T00:00:00Z"),
            numerator: d("2"),
            denominator: d("1"),
            broker_confirmed: true,
            available_at: time("2026-10-02T00:00:00Z"),
            reference: "confirmed-split".into(),
        });
        input.sell_replace = None;
        input.sell_replace_unavailable_reason = Some("outside this fixture".into());
        for hundredths in [1, 7, 33, 99, 100, 225, 399, 400, 599, 600] {
            let target = input.hold_to_target.as_mut().unwrap();
            target.quantity = Decimal::new(hundredths, 2);
            target.unit_proceeds = d("50");
            target.sale_fees = Decimal::ZERO;
            target.remaining_position_value_tax_currency =
                mul(sub(d("6"), target.quantity).unwrap(), d("50")).unwrap();
            let result = assess(&input, at()).unwrap();
            let allocations = &result.hold_to_target.unwrap().lot_allocations;
            let mut basis = Decimal::ZERO;
            let mut quantity = Decimal::ZERO;
            let mut sold = Decimal::ZERO;
            for lot in allocations {
                basis = add(
                    basis,
                    add(
                        lot.consumed_basis_tax_currency,
                        lot.remaining_basis_tax_currency,
                    )
                    .unwrap(),
                )
                .unwrap();
                quantity = add(
                    quantity,
                    add(lot.sold_quantity, lot.remaining_quantity).unwrap(),
                )
                .unwrap();
                sold = add(sold, lot.sold_quantity).unwrap();
            }
            assert_eq!(basis, d("153"));
            assert_eq!(quantity, d("6"));
            assert_eq!(sold, Decimal::new(hundredths, 2));
        }
    }

    #[test]
    fn best_lot_is_rejected_unless_specific_identification_is_explicitly_permitted() {
        let mut input = fixture();
        input.hold_to_target.as_mut().unwrap().selected_lot_ids = vec!["new-lot".into()];
        assert!(assess(&input, at()).is_err());
        input.rules.as_mut().unwrap().lot_method = TaxLotMethod::SpecificIdentification;
        assert_eq!(
            assess(&input, at()).unwrap().status,
            TaxAssessmentStatus::EstimateUnavailable
        );
        input
            .rules
            .as_mut()
            .unwrap()
            .specific_identification_permitted = true;
        input.sell_replace = None;
        input.sell_replace_unavailable_reason = Some("not compared".into());
        let target = assess(&input, at()).unwrap().hold_to_target.unwrap();
        assert_eq!(target.realized_gain_tax_currency, d("27"));
        assert_eq!(target.incremental_final_liability.central, d("6.75"));
    }

    #[test]
    fn acquisition_and_disposal_fx_use_documented_required_valuation_dates() {
        let mut input = fixture();
        let usd = Currency::new("USD").unwrap();
        let lot = &mut input.lots.as_mut().unwrap()[0];
        lot.currency = usd.clone();
        lot.acquisition_fx = Some(TaxFxRate {
            from: usd.clone(),
            to: eur(),
            rate: d("0.9"),
            valuation_date: lot.acquired_at.date_naive(),
            source_published_at: lot.acquired_at,
            available_at: lot.available_at,
            reference: "acquisition-fx".into(),
            owner_reviewed: true,
            illustrative_assumption: false,
        });
        let target = input.hold_to_target.as_mut().unwrap();
        target.currency = usd.clone();
        target.disposal_fx = Some(TaxFxRate {
            from: usd,
            to: eur(),
            rate: d("0.8"),
            valuation_date: target.disposed_at.date_naive(),
            source_published_at: at() - Duration::hours(1),
            available_at: at(),
            reference: "explicit-future-fx-assumption".into(),
            owner_reviewed: true,
            illustrative_assumption: true,
        });
        let result = assess(&input, at()).unwrap().hold_to_target.unwrap();
        assert_eq!(result.realized_gain_tax_currency, d("41.5"));
        assert_eq!(result.after_tax_value_tax_currency.central, d("248.025"));
        input
            .hold_to_target
            .as_mut()
            .unwrap()
            .disposal_fx
            .as_mut()
            .unwrap()
            .valuation_date = at().date_naive();
        assert!(assess(&input, at()).is_err());
        input.lots.as_mut().unwrap()[0].acquisition_fx = None;
        assert!(assess(&input, at()).is_err());
    }

    #[test]
    fn same_withholding_in_net_cash_and_duplicate_annual_reports_is_counted_exactly_once() {
        let mut input = fixture();
        let withholding = TaxWithholding {
            event_id: "one-payment".into(),
            related_disposal_id: Some("target".into()),
            amount_tax_currency: d("10"),
            paid_at: at() - Duration::minutes(1),
            available_at: at(),
            reference: "broker-cash".into(),
            broker_confirmed: true,
            illustrative_assumption: false,
        };
        let mut annual_copy = withholding.clone();
        annual_copy.reference = "annual-statement".into();
        input.annual_state.as_mut().unwrap().withholding = vec![withholding.clone(), annual_copy];
        let target = input.hold_to_target.as_mut().unwrap();
        target.disposed_at = at();
        target.tax_valuation_date = at().date_naive();
        target.withholding = Some(withholding);
        target.withholding_confirmed_absent = false;
        target.broker_cash = Some(TaxCashObservation {
            amount_tax_currency: d("88"),
            basis: TaxCashBasis::NetAfterWithholding,
            available_at: at(),
            reference: "actual-cash".into(),
        });
        let result = assess(&input, at()).unwrap().hold_to_target.unwrap();
        assert_eq!(result.immediate_sale_cash_tax_currency, d("88"));
        assert_eq!(result.estimated_annual_liability.central, d("14.25"));
        assert_eq!(result.incremental_annual_settlement.central, d("4.25"));
        assert_eq!(result.after_tax_value_tax_currency.central, d("263.75"));
        input
            .hold_to_target
            .as_mut()
            .unwrap()
            .broker_cash
            .as_mut()
            .unwrap()
            .amount_tax_currency = d("78");
        assert!(assess(&input, at()).is_err());
    }

    #[test]
    fn conflicting_withholding_is_rejected_and_unknown_withholding_is_not_zero() {
        let mut input = fixture();
        let target = input.hold_to_target.as_mut().unwrap();
        target.withholding_confirmed_absent = false;
        target.withholding_unavailable_reason = Some("broker deduction unconfirmed".into());
        let result = assess(&input, at()).unwrap();
        assert!(result.hold_to_target.is_none());
        assert_eq!(result.status, TaxAssessmentStatus::Partial);
        let one = TaxWithholding {
            event_id: "dup".into(),
            related_disposal_id: None,
            amount_tax_currency: d("1"),
            paid_at: at(),
            available_at: at(),
            reference: "one".into(),
            broker_confirmed: true,
            illustrative_assumption: false,
        };
        let mut other = one.clone();
        other.amount_tax_currency = d("2");
        input.annual_state.as_mut().unwrap().withholding = vec![one, other];
        assert!(assess(&input, at()).is_err());
    }

    #[test]
    fn approved_loss_offsets_dividends_allowance_and_carryforward_are_explicit_scenarios() {
        let mut input = fixture();
        let annual = input.annual_state.as_mut().unwrap();
        annual.gross_stock_gains = Some(d("20"));
        annual.stock_losses = Some(d("10"));
        annual.gross_dividends = Some(d("30"));
        annual.confirmed_allowance = Some(d("5"));
        annual.confirmed_loss_carryforward = Some(d("3"));
        let target = input.hold_to_target.as_mut().unwrap();
        target.unit_proceeds = d("5");
        target.sale_fees = Decimal::ZERO;
        let result = assess(&input, at()).unwrap();
        assert_eq!(
            result.no_action.unwrap().estimated_annual_liability.central,
            d("8")
        );
        let target = result.hold_to_target.unwrap();
        assert_eq!(target.realized_gain_tax_currency, d("-36"));
        assert_eq!(target.estimated_annual_liability.central, Decimal::ZERO);
        assert_eq!(target.incremental_final_liability.central, d("-8"));
        assert_eq!(target.after_tax_value_tax_currency.central, d("193"));
        input.rules.as_mut().unwrap().loss_treatment = TaxLossTreatment::GainsOnly;
        let target = assess(&input, at()).unwrap().hold_to_target.unwrap();
        assert_eq!(target.incremental_final_liability.central, d("-1.75"));
        assert_eq!(target.after_tax_value_tax_currency.central, d("186.75"));
    }

    #[test]
    fn latched_stops_protective_exits_consumed_slots_and_risk_gates_remain_unchanged() {
        for mode in 0..6 {
            let mut input = fixture();
            match mode {
                0 => input.safety.exit_latched = true,
                1 => input.safety.protective_exit_active = true,
                2 => input.safety.entry_slot_available = false,
                3 => input.safety.replacement_path_approved = false,
                4 => input.safety.replacement_risk_valid = false,
                _ => input.safety.leverage_enabled = true,
            }
            let before = serde_json::to_value(&input).unwrap();
            let result = assess(&input, at()).unwrap();
            assert!(result.sell_replace.is_none());
            assert_eq!(result.preserved_safety, input.safety);
            assert_eq!(serde_json::to_value(&input).unwrap(), before);
            assert_eq!(result.preserved_safety.original_stop, d("80"));
            assert_eq!(result.preserved_safety.original_target, d("100"));
            assert_eq!(result.preserved_safety.confidence_gate, 90);
        }
    }

    #[test]
    fn rule_updates_invalidate_prospective_estimates_without_rewriting_history() {
        let mut input = fixture();
        let old = assess(&input, at()).unwrap();
        input.rules.as_mut().unwrap().version = "synthetic-rules-v2".into();
        input.rules.as_mut().unwrap().gains_rate = rates("0.30");
        let new = assess(&input, at()).unwrap();
        assert_eq!(
            old.assumptions.as_ref().unwrap().rule_version,
            "synthetic-rules-v1"
        );
        assert_eq!(
            old.hold_to_target
                .unwrap()
                .incremental_final_liability
                .central,
            d("14.25")
        );
        assert_eq!(
            new.hold_to_target
                .unwrap()
                .incremental_final_liability
                .central,
            d("17.1")
        );
        input.rules.as_mut().unwrap().effective_until = at();
        let expired = assess(&input, at()).unwrap();
        assert_eq!(expired.status, TaxAssessmentStatus::EstimateUnavailable);
        assert!(expired.assumptions.is_none());
    }

    #[test]
    fn rate_ranges_remain_ranges_and_financial_overflow_is_rejected() {
        let mut input = fixture();
        input.rules.as_mut().unwrap().gains_rate = TaxRateRange {
            lower: d("0.2"),
            central: d("0.25"),
            upper: d("0.3"),
        };
        let target = assess(&input, at()).unwrap().hold_to_target.unwrap();
        assert_eq!(target.incremental_final_liability.lower, d("11.4"));
        assert_eq!(target.incremental_final_liability.upper, d("17.1"));
        assert_eq!(target.after_tax_value_tax_currency.lower, d("260.9"));
        assert_eq!(target.after_tax_value_tax_currency.upper, d("266.6"));
        input.lots.as_mut().unwrap()[0].quantity = Decimal::MAX;
        assert!(assess(&input, at()).is_err());
    }

    #[test]
    fn assessments_exclude_raw_context_identity_and_contracts_reject_unknown_fields() {
        let input = fixture();
        let result = assess(&input, at()).unwrap();
        let json = serde_json::to_string(&result).unwrap();
        for private in [
            "SYNTHETIC_JURISDICTION",
            "private-broker-entity",
            "private-statement-reference",
            "private-annual-statement",
            "account_wrapper",
            "residence_owner_confirmed",
        ] {
            assert!(!json.contains(private));
        }
        let mut encoded = serde_json::to_value(input).unwrap();
        assert_eq!(encoded["lots"][0]["quantity"], "2");
        encoded["order_authority"] = true.into();
        assert!(serde_json::from_value::<TaxInput>(encoded).is_err());
    }

    #[test]
    fn owner_prose_and_unvalidated_blocked_scenario_assumptions_are_not_echoed() {
        let mut input = fixture();
        input.context = None;
        input.context_unavailable_reason = Some("private owner identity in free text".into());
        let result = assess(&input, at()).unwrap();
        assert!(
            !serde_json::to_string(&result)
                .unwrap()
                .contains("private owner identity")
        );
        input = fixture();
        input.safety.exit_latched = true;
        input.sell_replace.as_mut().unwrap().disposal.disposal_fx = Some(TaxFxRate {
            from: Currency::new("USD").unwrap(),
            to: eur(),
            rate: d("0.8"),
            valuation_date: at().date_naive(),
            source_published_at: at(),
            available_at: at(),
            reference: "unvalidated_blocked_reference".into(),
            owner_reviewed: false,
            illustrative_assumption: true,
        });
        let result = assess(&input, at()).unwrap();
        assert!(
            result
                .assumptions
                .as_ref()
                .unwrap()
                .fx_observations_and_assumptions
                .is_empty()
        );
        assert!(
            !serde_json::to_string(&result)
                .unwrap()
                .contains("unvalidated_blocked_reference")
        );
    }

    #[test]
    fn approved_tax_valuation_dates_are_not_inferred_from_utc_or_execution_date() {
        let mut input = fixture();
        let usd = Currency::new("USD").unwrap();
        let lot = &mut input.lots.as_mut().unwrap()[0];
        lot.currency = usd.clone();
        lot.acquired_at = time("2026-01-10T00:30:00Z");
        lot.tax_valuation_date = time("2026-01-09T00:00:00Z").date_naive();
        lot.acquisition_fx = Some(TaxFxRate {
            from: usd,
            to: eur(),
            rate: d("0.9"),
            valuation_date: lot.tax_valuation_date,
            source_published_at: time("2026-01-09T00:00:00Z"),
            available_at: lot.available_at,
            reference: "owner-reviewed-local-tax-date".into(),
            owner_reviewed: true,
            illustrative_assumption: false,
        });
        let result = assess(&input, at()).unwrap();
        assert_eq!(
            result.hold_to_target.unwrap().realized_gain_tax_currency,
            d("61.1")
        );
        input.lots.as_mut().unwrap()[0]
            .acquisition_fx
            .as_mut()
            .unwrap()
            .valuation_date = time("2026-01-10T00:00:00Z").date_naive();
        assert!(assess(&input, at()).is_err());
    }

    #[test]
    fn declared_tax_year_can_differ_from_utc_calendar_year_at_the_boundary() {
        let mut input = fixture();
        let now = time("2027-01-01T00:30:00Z");
        input.assessment_tax_date = time("2026-12-31T00:00:00Z").date_naive();
        input.comparison_tax_date = input.assessment_tax_date;
        input.comparison_horizon = time("2027-01-01T01:00:00Z");
        input.context.as_mut().unwrap().valid_until = time("2027-01-02T00:00:00Z");
        input.rules.as_mut().unwrap().effective_until = time("2027-01-01T05:00:00Z");
        let target = input.hold_to_target.as_mut().unwrap();
        target.disposed_at = input.comparison_horizon;
        target.tax_valuation_date = input.comparison_tax_date;
        input.sell_replace = None;
        input.sell_replace_unavailable_reason = Some("no replacement path".into());
        let result = assess(&input, now).unwrap();
        assert!(result.hold_to_target.is_some());
        assert_eq!(result.assumptions.unwrap().assessment_tax_date.year(), 2026);
    }
}

//! Checked accounting ratios with per-ratio abstention and matched empirical ranks.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use std::collections::BTreeMap;
use trady_domain::intelligence::accounting::*;
use trady_domain::{Error, Result, require};

const VERSION: &str = "earnings-quality-v1";

pub fn assess(input: &AccountingInput, at: DateTime<Utc>) -> Result<AccountingAssessment> {
    require(
        !input.issuer_id.is_empty()
            && !input.crosswalk_revision.is_empty()
            && !input.accounting_basis.is_empty()
            && !input.consolidated_scope.is_empty()
            && !input.cohort.industry_id.is_empty()
            && !input.cohort.model_revision.is_empty(),
        "missing accounting semantics",
    )?;
    validate_period(&input.current_period)?;
    validate_period(&input.comparison_period)?;
    require(
        input.current_period.kind != FinancialPeriodKind::Instant
            && input.comparison_period.end
                < input
                    .current_period
                    .start
                    .ok_or_else(|| Error("missing current start".into()))?
            && input.current_period.end <= at.date_naive(),
        "invalid accounting comparison chronology",
    )?;
    require(
        input.minimum_denominator > Decimal::ZERO,
        "accounting denominator floor must be positive",
    )?;
    require(
        input.minimum_cohort_size >= 2,
        "accounting cohort needs at least two other issuers",
    )?;

    let mut facts = BTreeMap::new();
    for fact in &input.facts {
        require(
            fact.issuer_id == input.issuer_id
                && !fact.fact_id.is_empty()
                && !fact.filing_accession.is_empty()
                && fact.filed_at <= fact.available_at
                && fact.period.end <= fact.filed_at.date_naive(),
            "invalid accounting fact identity or chronology",
        )?;
        validate_period(&fact.period)?;
        require(
            fact.value.is_some()
                || fact
                    .missing_reason
                    .as_ref()
                    .is_some_and(|s| !s.trim().is_empty()),
            "missing accounting value needs a reason",
        )?;
        if fact.available_at > at {
            continue;
        }
        if let Some(old) = facts.insert(fact.fact_id.as_str(), fact) {
            require(old == fact, "conflicting duplicate accounting fact id")?;
        }
    }
    let mut active = facts.clone();
    let mut revisions = Vec::new();
    let mut unresolved_revisions = Vec::new();
    for fact in facts.values() {
        if let Some(original_id) = &fact.revision_of {
            if let Some(original) = facts.get(original_id.as_str()) {
                require(
                    original.available_at < fact.available_at
                        && original.concept == fact.concept
                        && same_period(&original.period, &fact.period),
                    "accounting revision must identify an earlier fact of the same concept and period",
                )?;
                active.remove(original_id.as_str());
                revisions.push(FinancialFactRevision {
                    original_fact_id: original_id.clone(),
                    replacement_fact_id: fact.fact_id.clone(),
                    available_at: fact.available_at,
                    source_ref: fact.source_ref.clone(),
                });
            } else {
                unresolved_revisions.push(fact.fact_id.clone());
            }
        }
    }
    revisions.sort_by(|a, b| {
        (&a.available_at, &a.replacement_fact_id).cmp(&(&b.available_at, &b.replacement_fact_id))
    });
    let context = Calculation {
        input,
        at,
        facts: active.values().copied().collect(),
        unresolved_revisions,
    };
    let mut signals = Vec::new();
    for metric in [
        AccountingMetric::AccrualsToAverageAssets,
        AccountingMetric::CashConversion,
        AccountingMetric::ReceivablesRevenueGrowthGap,
        AccountingMetric::InventoryRevenueGrowthGap,
        AccountingMetric::DilutedWeightedShareGrowth,
        AccountingMetric::OutstandingShareGrowth,
    ] {
        let mut signal = AnomalySignal {
            metric,
            formula_revision: VERSION.into(),
            formula: formula(metric).into(),
            value: None,
            fact_ids: vec![],
            abstention_reasons: vec![],
            empirical_percentile: None,
            cohort_size: 0,
            rank_abstention_reason: None,
            cohort_source_refs: vec![],
        };
        let issues: Vec<_> = input
            .comparability_issues
            .iter()
            .filter(|i| i.available_at <= at && i.metrics.contains(&metric))
            .collect();
        for issue in issues {
            require(
                !issue.reason.is_empty() && !issue.source_ref.is_empty(),
                "unreferenced accounting comparability issue",
            )?;
            signal.abstention_reasons.push(issue.reason.clone());
        }
        if signal.abstention_reasons.is_empty() {
            match context.calculate(metric, &mut signal.fact_ids) {
                Ok(value) => signal.value = Some(value),
                Err(reason) => signal.abstention_reasons.push(reason),
            }
        }
        rank(input, at, &mut signal)?;
        signals.push(signal);
    }
    let mut reasons = vec![
        "ratios_and_empirical_ranks_are_research_context_not_fraud_or_return_probabilities".into(),
        "long_horizon_accounting_features_require_separate_short_horizon_validation".into(),
    ];
    if !revisions.is_empty() {
        reasons.push(
            "available_restatement_revisions_require_dependent_assessment_invalidation".into(),
        );
    }
    let one_off_disclosures = context
        .facts
        .iter()
        .filter(|f| {
            matches!(
                f.concept,
                FinancialConcept::Impairment
                    | FinancialConcept::Restructuring
                    | FinancialConcept::PensionOneOff
                    | FinancialConcept::AcquisitionOneOff
            )
        })
        .map(|f| (*f).clone())
        .collect();
    Ok(AccountingAssessment {
        issuer_id: input.issuer_id.clone(),
        as_of: at,
        version: VERSION.into(),
        crosswalk_revision: input.crosswalk_revision.clone(),
        current_period: input.current_period.clone(),
        comparison_period: input.comparison_period.clone(),
        cohort: input.cohort.clone(),
        signals,
        restatements: revisions,
        one_off_disclosures,
        reasons,
    })
}

fn validate_period(p: &FinancialPeriod) -> Result<()> {
    require(!p.fiscal_label.is_empty(), "missing fiscal period label")?;
    if p.kind == FinancialPeriodKind::Instant {
        require(
            p.start.is_none() && p.fiscal_weeks.is_none(),
            "instant fact has duration metadata",
        )
    } else {
        require(
            p.start.is_some_and(|s| s <= p.end) && p.fiscal_weeks.is_none_or(|w| w > 0 && w <= 54),
            "invalid financial duration",
        )
    }
}

fn same_period(a: &FinancialPeriod, b: &FinancialPeriod) -> bool {
    a.kind == b.kind && a.start == b.start && a.end == b.end && a.fiscal_weeks == b.fiscal_weeks
}

fn instant(end: NaiveDate) -> FinancialPeriod {
    FinancialPeriod {
        fiscal_label: end.to_string(),
        kind: FinancialPeriodKind::Instant,
        start: None,
        end,
        fiscal_weeks: None,
    }
}

fn formula(m: AccountingMetric) -> &'static str {
    match m {
        AccountingMetric::AccrualsToAverageAssets => {
            "(net_income - operating_cash_flow) / ((opening_assets + closing_assets) / 2)"
        }
        AccountingMetric::CashConversion => "operating_cash_flow / materially_positive_net_income",
        AccountingMetric::ReceivablesRevenueGrowthGap => {
            "(receivables_current / receivables_prior - 1) - (revenue_current / revenue_prior - 1)"
        }
        AccountingMetric::InventoryRevenueGrowthGap => {
            "(inventory_current / inventory_prior - 1) - (revenue_current / revenue_prior - 1)"
        }
        AccountingMetric::DilutedWeightedShareGrowth => {
            "split_adjusted_diluted_weighted_shares_current / split_adjusted_diluted_weighted_shares_prior - 1"
        }
        AccountingMetric::OutstandingShareGrowth => {
            "split_adjusted_outstanding_shares_current / split_adjusted_outstanding_shares_prior - 1"
        }
    }
}

struct Calculation<'a> {
    input: &'a AccountingInput,
    at: DateTime<Utc>,
    facts: Vec<&'a ComparableFinancialFact>,
    unresolved_revisions: Vec<String>,
}

type RatioResult<T> = std::result::Result<T, String>;

impl<'a> Calculation<'a> {
    fn fact(
        &self,
        concept: FinancialConcept,
        period: &FinancialPeriod,
        ids: &mut Vec<String>,
    ) -> RatioResult<(&'a ComparableFinancialFact, Decimal)> {
        let candidates: Vec<_> = self
            .facts
            .iter()
            .filter(|f| f.concept == concept && same_period(&f.period, period))
            .copied()
            .collect();
        ids.extend(candidates.iter().map(|f| f.fact_id.clone()));
        if candidates.is_empty() {
            return Err(format!("missing_or_misaligned_{concept:?}_period"));
        }
        if candidates.len() != 1 {
            return Err(format!("ambiguous_duplicate_taxonomy_facts_{concept:?}"));
        }
        let f = candidates[0];
        if self.unresolved_revisions.contains(&f.fact_id) {
            return Err("missing_original_revision_fact".into());
        }
        if f.source_ref.as_ref().is_none_or(|s| s.is_empty()) || f.label_as_filed.is_empty() {
            return Err("missing_original_filing_evidence".into());
        }
        if f.taxonomy_tag.as_ref().is_none_or(|t| t.is_empty())
            || f.crosswalk_revision.as_ref() != Some(&self.input.crosswalk_revision)
        {
            return Err("missing_or_different_taxonomy_crosswalk".into());
        }
        if f.accounting_basis != self.input.accounting_basis
            || f.consolidated_scope != self.input.consolidated_scope
        {
            return Err("accounting_basis_or_consolidated_scope_mismatch".into());
        }
        if f.unit.scale <= Decimal::ZERO {
            return Err("invalid_unit_scale".into());
        }
        let value = f.value.ok_or_else(|| {
            f.missing_reason
                .clone()
                .unwrap_or_else(|| "missing_fact_value".into())
        })?;
        Ok((f, checked_mul(value, f.unit.scale)?))
    }

    fn comparable_money(&self, facts: &[&ComparableFinancialFact]) -> RatioResult<()> {
        let currency = facts
            .first()
            .and_then(|f| f.unit.currency.as_deref())
            .ok_or("missing_accounting_currency")?;
        if currency.len() != 3
            || !currency.bytes().all(|b| b.is_ascii_uppercase())
            || facts.iter().any(|f| {
                f.unit.measure != "currency" || f.unit.currency.as_deref() != Some(currency)
            })
        {
            return Err("currency_or_economic_unit_mismatch".into());
        }
        Ok(())
    }

    fn denominator(&self, value: Decimal) -> RatioResult<Decimal> {
        if value <= self.input.minimum_denominator {
            Err("nonpositive_or_immaterial_denominator".into())
        } else {
            Ok(value)
        }
    }

    fn comparable_growth_periods(&self) -> RatioResult<()> {
        let (a, b) = (&self.input.current_period, &self.input.comparison_period);
        if a.kind != b.kind || a.fiscal_weeks.is_none() || a.fiscal_weeks != b.fiscal_weeks {
            return Err("period_kind_or_fiscal_week_mismatch_including_53_week_year".into());
        }
        Ok(())
    }

    fn calculate(&self, metric: AccountingMetric, ids: &mut Vec<String>) -> RatioResult<Decimal> {
        use AccountingMetric as M;
        use FinancialConcept as C;
        let current = &self.input.current_period;
        let previous = &self.input.comparison_period;
        match metric {
            M::AccrualsToAverageAssets => {
                let (income, ni) = self.fact(C::NetIncome, current, ids)?;
                let (cash, ocf) = self.fact(C::OperatingCashFlow, current, ids)?;
                let opening_date = current
                    .start
                    .and_then(|d| d.pred_opt())
                    .ok_or("missing_opening_assets_date")?;
                let (opening, a0) = self.fact(C::Assets, &instant(opening_date), ids)?;
                let (closing, a1) = self.fact(C::Assets, &instant(current.end), ids)?;
                self.comparable_money(&[income, cash, opening, closing])?;
                if a0 < Decimal::ZERO || a1 < Decimal::ZERO {
                    return Err("negative_asset_balance".into());
                }
                let average = checked_div(checked_add(a0, a1)?, Decimal::from(2))?;
                checked_div(checked_sub(ni, ocf)?, self.denominator(average)?)
            }
            M::CashConversion => {
                let (income, ni) = self.fact(C::NetIncome, current, ids)?;
                let (cash, ocf) = self.fact(C::OperatingCashFlow, current, ids)?;
                self.comparable_money(&[income, cash])?;
                checked_div(ocf, self.denominator(ni)?)
            }
            M::ReceivablesRevenueGrowthGap | M::InventoryRevenueGrowthGap => {
                if metric == M::InventoryRevenueGrowthGap && !self.input.cohort.inventory_relevant {
                    return Err("inventory_not_comparable_for_this_industry_model".into());
                }
                self.comparable_growth_periods()?;
                let concept = if metric == M::ReceivablesRevenueGrowthGap {
                    C::TradeReceivables
                } else {
                    C::Inventory
                };
                let (balance_now, b1) = self.fact(concept, &instant(current.end), ids)?;
                let (balance_before, b0) = self.fact(concept, &instant(previous.end), ids)?;
                let (revenue_now, r1) = self.fact(C::Revenue, current, ids)?;
                let (revenue_before, r0) = self.fact(C::Revenue, previous, ids)?;
                self.comparable_money(&[balance_now, balance_before, revenue_now, revenue_before])?;
                if b1 < Decimal::ZERO || r1 < Decimal::ZERO {
                    return Err("negative_balance_or_revenue_not_comparable".into());
                }
                checked_sub(
                    checked_div(b1, self.denominator(b0)?)?,
                    checked_div(r1, self.denominator(r0)?)?,
                )
            }
            M::DilutedWeightedShareGrowth | M::OutstandingShareGrowth => {
                self.comparable_growth_periods()?;
                let concept = if metric == M::DilutedWeightedShareGrowth {
                    C::DilutedWeightedAverageShares
                } else {
                    C::OutstandingShares
                };
                let now_period = if metric == M::OutstandingShareGrowth {
                    instant(current.end)
                } else {
                    current.clone()
                };
                let before_period = if metric == M::OutstandingShareGrowth {
                    instant(previous.end)
                } else {
                    previous.clone()
                };
                let (now_fact, n1) = self.fact(concept, &now_period, ids)?;
                let (before_fact, n0) = self.fact(concept, &before_period, ids)?;
                if [now_fact, before_fact]
                    .iter()
                    .any(|f| f.unit.measure != "shares" || f.unit.currency.is_some())
                {
                    return Err("share_unit_mismatch".into());
                }
                let a = now_fact
                    .share_adjustment
                    .as_ref()
                    .ok_or("missing_current_split_adjustment_evidence")?;
                let b = before_fact
                    .share_adjustment
                    .as_ref()
                    .ok_or("missing_prior_split_adjustment_evidence")?;
                if a.factor <= Decimal::ZERO
                    || b.factor <= Decimal::ZERO
                    || a.available_at > self.at
                    || b.available_at > self.at
                    || a.basis_date != b.basis_date
                    || a.basis_date > self.at.date_naive()
                    || a.source_ref.is_empty()
                    || b.source_ref.is_empty()
                {
                    return Err("unavailable_or_inconsistent_split_adjustment_basis".into());
                }
                if n1 < Decimal::ZERO {
                    return Err("negative_share_count".into());
                }
                checked_sub(
                    checked_div(
                        checked_mul(n1, a.factor)?,
                        self.denominator(checked_mul(n0, b.factor)?)?,
                    )?,
                    Decimal::ONE,
                )
            }
        }
    }
}

fn rank(input: &AccountingInput, at: DateTime<Utc>, signal: &mut AnomalySignal) -> Result<()> {
    let Some(value) = signal.value else {
        signal.rank_abstention_reason = Some("ratio_unavailable".into());
        return Ok(());
    };
    if input.current_period.fiscal_weeks.is_none() {
        signal.rank_abstention_reason = Some("cohort_requires_fiscal_duration".into());
        return Ok(());
    }
    let mut peers: BTreeMap<&str, Option<&AccountingPeerObservation>> = BTreeMap::new();
    for peer in &input.peers {
        if peer.issuer_id == input.issuer_id
            || peer.issuer_id.is_empty()
            || peer.available_at > at
            || peer.metric != signal.metric
            || !peer.comparable
            || peer.industry_id != input.cohort.industry_id
            || peer.cohort_model_revision != input.cohort.model_revision
            || peer.formula_revision != VERSION
            || peer.crosswalk_revision != input.crosswalk_revision
            || peer.period_kind != input.current_period.kind
            || peer.fiscal_weeks != input.current_period.fiscal_weeks
            || peer.period_end != input.current_period.end
            || peer.period_end > peer.available_at.date_naive()
            || peer.accounting_basis != input.accounting_basis
            || peer.consolidated_scope != input.consolidated_scope
            || peer.value.is_none()
            || peer.source_refs.is_empty()
            || peer.source_refs.iter().any(|s| s.is_empty())
        {
            continue;
        }
        match peers.entry(&peer.issuer_id) {
            std::collections::btree_map::Entry::Vacant(e) => {
                e.insert(Some(peer));
            }
            std::collections::btree_map::Entry::Occupied(mut e) => {
                if e.get().is_some_and(|old| old != peer) {
                    e.insert(None);
                }
            }
        }
    }
    let peers: Vec<_> = peers.into_values().flatten().collect();
    signal.cohort_size =
        u32::try_from(peers.len()).map_err(|_| Error("too many accounting peers".into()))?;
    if signal.cohort_size < input.minimum_cohort_size {
        signal.rank_abstention_reason = Some("insufficient_comparable_industry_cohort".into());
        return Ok(());
    }
    let less = peers
        .iter()
        .filter(|p| p.value.is_some_and(|v| v < value))
        .count();
    let equal = peers.iter().filter(|p| p.value == Some(value)).count();
    let less =
        Decimal::from(u64::try_from(less).map_err(|_| Error("cohort count overflow".into()))?);
    let equal =
        Decimal::from(u64::try_from(equal).map_err(|_| Error("cohort count overflow".into()))?);
    signal.empirical_percentile = less
        .checked_add(
            equal
                .checked_div(Decimal::from(2))
                .ok_or_else(|| Error("rank division overflow".into()))?,
        )
        .and_then(|v| v.checked_div(Decimal::from(signal.cohort_size)));
    require(
        signal.empirical_percentile.is_some(),
        "empirical rank overflow",
    )?;
    signal.cohort_source_refs = peers
        .iter()
        .flat_map(|p| p.source_refs.iter().cloned())
        .collect();
    signal.cohort_source_refs.sort();
    signal.cohort_source_refs.dedup();
    Ok(())
}

fn checked_add(a: Decimal, b: Decimal) -> RatioResult<Decimal> {
    a.checked_add(b).ok_or("decimal_addition_overflow".into())
}
fn checked_sub(a: Decimal, b: Decimal) -> RatioResult<Decimal> {
    a.checked_sub(b)
        .ok_or("decimal_subtraction_overflow".into())
}
fn checked_mul(a: Decimal, b: Decimal) -> RatioResult<Decimal> {
    a.checked_mul(b)
        .ok_or("decimal_multiplication_overflow".into())
}
fn checked_div(a: Decimal, b: Decimal) -> RatioResult<Decimal> {
    a.checked_div(b)
        .ok_or("decimal_division_overflow_or_zero".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn time() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-10-30T21:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }
    fn date(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }
    fn period(year: i32) -> FinancialPeriod {
        FinancialPeriod {
            fiscal_label: format!("{year}-Q3"),
            kind: FinancialPeriodKind::Quarter,
            start: Some(date(&format!("{year}-07-01"))),
            end: date(&format!("{year}-09-30")),
            fiscal_weeks: Some(13),
        }
    }
    fn fact(
        id: &str,
        concept: FinancialConcept,
        p: FinancialPeriod,
        value: i64,
    ) -> ComparableFinancialFact {
        let shares = matches!(
            concept,
            FinancialConcept::DilutedWeightedAverageShares | FinancialConcept::OutstandingShares
        );
        ComparableFinancialFact {
            fact_id: id.into(),
            issuer_id: "issuer".into(),
            concept,
            label_as_filed: format!("{concept:?}"),
            taxonomy_tag: Some(format!("us-gaap:{concept:?}")),
            crosswalk_revision: Some("crosswalk-v1".into()),
            period: p,
            unit: FinancialUnit {
                measure: if shares { "shares" } else { "currency" }.into(),
                currency: if shares { None } else { Some("USD".into()) },
                scale: Decimal::ONE,
            },
            accounting_basis: "GAAP".into(),
            consolidated_scope: "consolidated".into(),
            value: Some(Decimal::from(value)),
            missing_reason: None,
            filing_accession: "filing".into(),
            filed_at: time(),
            available_at: time(),
            revision_of: None,
            source_ref: Some(format!("sec:filing#{id}")),
            source_text_offset: Some(10),
            share_adjustment: shares.then_some(ShareCountAdjustment {
                factor: Decimal::ONE,
                basis_date: time().date_naive(),
                available_at: time(),
                source_ref: "actions:verified".into(),
            }),
        }
    }
    fn input() -> AccountingInput {
        use FinancialConcept as C;
        let current = period(2026);
        let previous = period(2025);
        let facts = vec![
            fact("ni", C::NetIncome, current.clone(), 20),
            fact("ocf", C::OperatingCashFlow, current.clone(), 10),
            fact("a0", C::Assets, instant(date("2026-06-30")), 100),
            fact("a1", C::Assets, instant(current.end), 100),
            fact("r1", C::Revenue, current.clone(), 120),
            fact("r0", C::Revenue, previous.clone(), 100),
            fact("ar1", C::TradeReceivables, instant(current.end), 30),
            fact("ar0", C::TradeReceivables, instant(previous.end), 20),
            fact("i1", C::Inventory, instant(current.end), 15),
            fact("i0", C::Inventory, instant(previous.end), 10),
            fact("w1", C::DilutedWeightedAverageShares, current.clone(), 110),
            fact("w0", C::DilutedWeightedAverageShares, previous.clone(), 100),
            fact("s1", C::OutstandingShares, instant(current.end), 110),
            fact("s0", C::OutstandingShares, instant(previous.end), 100),
        ];
        AccountingInput {
            issuer_id: "issuer".into(),
            current_period: current,
            comparison_period: previous,
            crosswalk_revision: "crosswalk-v1".into(),
            accounting_basis: "GAAP".into(),
            consolidated_scope: "consolidated".into(),
            cohort: AccountingCohort {
                industry_id: "manufacturer".into(),
                model_revision: "industry-v1".into(),
                inventory_relevant: true,
            },
            minimum_denominator: Decimal::new(1, 2),
            minimum_cohort_size: 2,
            facts,
            comparability_issues: vec![],
            peers: vec![],
        }
    }
    fn signal(a: &AccountingAssessment, m: AccountingMetric) -> &AnomalySignal {
        a.signals.iter().find(|s| s.metric == m).unwrap()
    }

    #[test]
    fn accounting_ratios_are_exact_and_rank_requires_empirical_industry_support() {
        let i = input();
        let a = assess(&i, time()).unwrap();
        assert_eq!(
            signal(&a, AccountingMetric::AccrualsToAverageAssets).value,
            Some(Decimal::new(1, 1))
        );
        assert_eq!(
            signal(&a, AccountingMetric::CashConversion).value,
            Some(Decimal::new(5, 1))
        );
        assert_eq!(
            signal(&a, AccountingMetric::ReceivablesRevenueGrowthGap).value,
            Some(Decimal::new(3, 1))
        );
        assert_eq!(
            signal(&a, AccountingMetric::DilutedWeightedShareGrowth).value,
            Some(Decimal::new(1, 1))
        );
        assert!(a.signals.iter().all(|s| s.empirical_percentile.is_none()));
    }

    #[test]
    fn accounting_ytd_cash_flow_does_not_match_quarter_earnings() {
        let mut i = input();
        let f = i.facts.iter_mut().find(|f| f.fact_id == "ocf").unwrap();
        f.period.kind = FinancialPeriodKind::YearToDate;
        f.period.start = Some(date("2026-01-01"));
        f.period.fiscal_weeks = Some(39);
        let a = assess(&i, time()).unwrap();
        assert!(signal(&a, AccountingMetric::CashConversion).value.is_none());
        assert!(
            signal(&a, AccountingMetric::AccrualsToAverageAssets)
                .value
                .is_none()
        );
        assert!(
            signal(&a, AccountingMetric::ReceivablesRevenueGrowthGap)
                .value
                .is_some()
        );
    }

    #[test]
    fn accounting_53_week_year_keeps_aligned_ratios_but_abstains_on_52_week_growth() {
        let mut i = input();
        let old_current = i.current_period.clone();
        let old_previous = i.comparison_period.clone();
        i.current_period = FinancialPeriod {
            fiscal_label: "2026-FY".into(),
            kind: FinancialPeriodKind::Annual,
            start: Some(date("2025-09-25")),
            end: date("2026-09-30"),
            fiscal_weeks: Some(53),
        };
        i.comparison_period = FinancialPeriod {
            fiscal_label: "2025-FY".into(),
            kind: FinancialPeriodKind::Annual,
            start: Some(date("2024-09-26")),
            end: date("2025-09-24"),
            fiscal_weeks: Some(52),
        };
        for f in &mut i.facts {
            if same_period(&f.period, &old_current) {
                f.period = i.current_period.clone();
            }
            if same_period(&f.period, &old_previous) {
                f.period = i.comparison_period.clone();
            }
            if f.fact_id == "a0" {
                f.period = instant(date("2025-09-24"));
            }
        }
        let a = assess(&i, time()).unwrap();
        assert!(signal(&a, AccountingMetric::CashConversion).value.is_some());
        assert!(
            signal(&a, AccountingMetric::AccrualsToAverageAssets)
                .value
                .is_some()
        );
        assert!(
            signal(&a, AccountingMetric::ReceivablesRevenueGrowthGap)
                .value
                .is_none()
        );
    }

    #[test]
    fn accounting_restatements_preserve_original_as_of_history() {
        let mut i = input();
        let mut revised = i.facts[0].clone();
        revised.fact_id = "ni-revised".into();
        revised.revision_of = Some("ni".into());
        revised.value = Some(Decimal::from(25));
        revised.filed_at += Duration::days(1);
        revised.available_at = revised.filed_at;
        i.facts.push(revised);
        let original = assess(&i, time()).unwrap();
        let updated = assess(&i, time() + Duration::days(1)).unwrap();
        assert!(original.restatements.is_empty());
        assert_eq!(updated.restatements.len(), 1);
        assert_eq!(
            signal(&original, AccountingMetric::CashConversion).value,
            Some(Decimal::new(5, 1))
        );
        assert_eq!(
            signal(&updated, AccountingMetric::CashConversion).value,
            Some(Decimal::new(4, 1))
        );
    }

    #[test]
    fn accounting_currency_acquisition_and_nonpositive_denominators_abstain_per_ratio() {
        let mut i = input();
        i.facts
            .iter_mut()
            .find(|f| f.fact_id == "r0")
            .unwrap()
            .unit
            .currency = Some("EUR".into());
        i.facts[0].value = Some(Decimal::ZERO);
        i.comparability_issues.push(AccountingComparabilityIssue {
            metrics: vec![AccountingMetric::InventoryRevenueGrowthGap],
            reason: "unresolved_acquisition_inventory_scope".into(),
            available_at: time(),
            source_ref: "sec:acquisition".into(),
        });
        let a = assess(&i, time()).unwrap();
        assert!(
            signal(&a, AccountingMetric::ReceivablesRevenueGrowthGap)
                .value
                .is_none()
        );
        assert!(
            signal(&a, AccountingMetric::InventoryRevenueGrowthGap)
                .value
                .is_none()
        );
        assert!(signal(&a, AccountingMetric::CashConversion).value.is_none());
        assert!(
            signal(&a, AccountingMetric::DilutedWeightedShareGrowth)
                .value
                .is_some()
        );
    }

    #[test]
    fn accounting_splits_do_not_fabricate_dilution_or_mix_share_definitions() {
        let mut i = input();
        for f in &mut i.facts {
            if f.fact_id == "w1" {
                f.value = Some(Decimal::from(200));
            }
            if f.fact_id == "w0" {
                f.share_adjustment.as_mut().unwrap().factor = Decimal::from(2);
            }
        }
        let a = assess(&i, time()).unwrap();
        assert_eq!(
            signal(&a, AccountingMetric::DilutedWeightedShareGrowth).value,
            Some(Decimal::ZERO)
        );
        assert_eq!(
            signal(&a, AccountingMetric::OutstandingShareGrowth).value,
            Some(Decimal::new(1, 1))
        );
        i.facts
            .iter_mut()
            .find(|f| f.fact_id == "w0")
            .unwrap()
            .share_adjustment = None;
        assert!(
            signal(
                &assess(&i, time()).unwrap(),
                AccountingMetric::DilutedWeightedShareGrowth
            )
            .value
            .is_none()
        );
    }

    #[test]
    fn accounting_duplicate_tags_missing_mapping_and_overflow_never_create_extreme_scores() {
        let mut i = input();
        let mut duplicate = i.facts[0].clone();
        duplicate.fact_id = "ni-extension".into();
        duplicate.taxonomy_tag = Some("issuer:Income".into());
        i.facts.push(duplicate);
        assert!(
            signal(
                &assess(&i, time()).unwrap(),
                AccountingMetric::CashConversion
            )
            .value
            .is_none()
        );
        i.facts.pop();
        i.facts[0].crosswalk_revision = None;
        assert!(
            signal(
                &assess(&i, time()).unwrap(),
                AccountingMetric::CashConversion
            )
            .value
            .is_none()
        );
        i.facts[0].crosswalk_revision = Some("crosswalk-v1".into());
        i.facts[0].value = Some(Decimal::MAX);
        i.facts[0].unit.scale = Decimal::from(1000);
        let a = assess(&i, time()).unwrap();
        assert!(signal(&a, AccountingMetric::CashConversion).value.is_none());
        assert!(
            signal(&a, AccountingMetric::CashConversion).abstention_reasons[0].contains("overflow")
        );
    }

    #[test]
    fn accounting_empirical_midrank_excludes_banks_duplicates_and_future_peers() {
        let mut i = input();
        for (issuer, value, industry) in [
            ("low", 0, "manufacturer"),
            ("high", 1, "manufacturer"),
            ("bank", 100, "bank"),
        ] {
            i.peers.push(AccountingPeerObservation {
                issuer_id: issuer.into(),
                industry_id: industry.into(),
                cohort_model_revision: "industry-v1".into(),
                metric: AccountingMetric::CashConversion,
                formula_revision: VERSION.into(),
                crosswalk_revision: "crosswalk-v1".into(),
                period_kind: FinancialPeriodKind::Quarter,
                fiscal_weeks: Some(13),
                period_end: i.current_period.end,
                accounting_basis: "GAAP".into(),
                consolidated_scope: "consolidated".into(),
                comparable: true,
                value: Some(Decimal::from(value)),
                missing_reason: None,
                available_at: time(),
                source_refs: vec![format!("sec:{issuer}")],
            });
        }
        i.peers.push(i.peers[0].clone());
        let a = assess(&i, time()).unwrap();
        let s = signal(&a, AccountingMetric::CashConversion);
        assert_eq!(s.cohort_size, 2);
        assert_eq!(s.empirical_percentile, Some(Decimal::new(5, 1)));
        i.peers[1].period_end -= Duration::days(365);
        assert!(
            signal(
                &assess(&i, time()).unwrap(),
                AccountingMetric::CashConversion
            )
            .empirical_percentile
            .is_none()
        );
        i.peers[1].period_end = i.current_period.end;
        i.peers[1].available_at += Duration::seconds(1);
        assert!(
            signal(
                &assess(&i, time()).unwrap(),
                AccountingMetric::CashConversion
            )
            .empirical_percentile
            .is_none()
        );
    }

    #[test]
    fn accounting_missing_original_source_and_loss_denominator_are_explicit() {
        let mut i = input();
        i.facts[0].source_ref = None;
        let a = assess(&i, time()).unwrap();
        assert_eq!(
            signal(&a, AccountingMetric::CashConversion).abstention_reasons,
            vec!["missing_original_filing_evidence"]
        );
        i.facts[0].source_ref = Some("sec:ni".into());
        i.facts[0].value = Some(-Decimal::ONE);
        let a = assess(&i, time()).unwrap();
        assert_eq!(
            signal(&a, AccountingMetric::CashConversion).abstention_reasons,
            vec!["nonpositive_or_immaterial_denominator"]
        );
    }

    #[test]
    fn issuer_events_fixture_accounting_has_strict_research_serialization() {
        let mut i = input();
        i.facts
            .retain(|f| ["ni", "ocf", "a0", "a1"].contains(&f.fact_id.as_str()));
        let mut json = serde_json::to_value(&i).unwrap();
        let decoded: AccountingInput = serde_json::from_value(json.clone()).unwrap();
        let assessment = assess(&decoded, time()).unwrap();
        assert_eq!(
            signal(&assessment, AccountingMetric::CashConversion).value,
            Some(Decimal::new(5, 1))
        );
        json.as_object_mut()
            .unwrap()
            .insert("order_authority".into(), serde_json::json!(true));
        assert!(serde_json::from_value::<AccountingInput>(json).is_err());
        let mut result = serde_json::to_value(&assessment).unwrap();
        assert_eq!(
            serde_json::from_value::<AccountingAssessment>(result.clone()).unwrap(),
            assessment
        );
        result
            .as_object_mut()
            .unwrap()
            .insert("expected_return".into(), serde_json::json!("0.5"));
        assert!(serde_json::from_value::<AccountingAssessment>(result).is_err());
        if std::env::var("TRADY_EMIT_ISSUER_EVENTS_FIXTURES").as_deref() == Ok("1") {
            println!(
                "PUBLIC_INPUT_FIXTURE accounting {}",
                serde_json::to_string(&i).unwrap()
            );
        }
    }
}

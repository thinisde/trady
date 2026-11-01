//! Publication-aligned, conservative Section 16 research transformations.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use std::collections::{BTreeMap, BTreeSet};
use trady_domain::intelligence::insider::*;
use trady_domain::{Error, Result, require};

pub fn assess(input: &InsiderInput, at: DateTime<Utc>) -> Result<InsiderAssessment> {
    require(!input.issuer_cik.is_empty(), "missing insider issuer")?;
    require(
        input.window_start <= input.window_end && input.window_end <= at,
        "invalid insider publication window",
    )?;
    require(
        input.reporting_currency.len() == 3
            && input
                .reporting_currency
                .bytes()
                .all(|v| v.is_ascii_uppercase()),
        "invalid insider reporting currency",
    )?;
    require(
        input.minimum_cluster_value > Decimal::ZERO,
        "cluster floor must be positive",
    )?;
    require(
        input.minimum_distinct_buyers >= 2,
        "a cluster requires distinct persons",
    )?;
    require(
        input.coverage == FilingCoverage::CompleteForDeclaredScope
            || input
                .coverage_reason
                .as_ref()
                .is_some_and(|s| !s.trim().is_empty()),
        "incomplete insider coverage needs a reason",
    )?;

    let mut filings = BTreeMap::new();
    for filing in &input.filings {
        require(
            filing.issuer_cik == input.issuer_cik,
            "mixed insider issuers",
        )?;
        require(
            !filing.accession.is_empty()
                && !filing.source_ref.is_empty()
                && !filing.source_content_hash.is_empty()
                && filing.filed_at <= filing.available_at,
            "invalid insider filing provenance",
        )?;
        // A future amendment must not affect the available original.
        if filing.available_at > input.window_end {
            continue;
        }
        if let Some(previous) = filings.insert(&filing.accession, filing) {
            require(previous == filing, "conflicting duplicate insider filing")?;
        }
    }
    let mut ordered: Vec<_> = filings.values().copied().collect();
    ordered.sort_by_key(|f| (f.available_at, f.filed_at, &f.accession));
    let mut active: BTreeMap<InsiderLineRef, (&OwnershipFiling, &InsiderTransaction)> =
        BTreeMap::new();
    let mut unresolved_amendment = false;
    for filing in ordered {
        let owner_ids: BTreeSet<_> = filing.owners.iter().map(|o| o.owner_id.as_str()).collect();
        require(
            owner_ids.len() == filing.owners.len() && !owner_ids.is_empty(),
            "ambiguous filing owners",
        )?;
        let mut local = BTreeMap::new();
        for row in &filing.transactions {
            require(
                !row.line_id.is_empty()
                    && !row.economic_id.is_empty()
                    && !row.source_ref.is_empty()
                    && !row.security_title.is_empty()
                    && !row.quantity_unit.is_empty()
                    && row.source_byte_start < row.source_byte_end
                    && row.transaction_date <= filing.filed_at.date_naive()
                    && !row.owner_ids.is_empty()
                    && row
                        .owner_ids
                        .iter()
                        .all(|id| owner_ids.contains(id.as_str()))
                    && row.owner_ids.iter().collect::<BTreeSet<_>>().len() == row.owner_ids.len(),
                "invalid insider transaction identity or chronology",
            )?;
            require(
                row.shares.is_none_or(|n| n >= Decimal::ZERO)
                    && row.price_per_unit.is_none_or(|n| n >= Decimal::ZERO),
                "negative ownership transaction amount",
            )?;
            require(
                row.footnote_ids
                    .iter()
                    .all(|id| filing.footnotes.iter().any(|f| &f.id == id)),
                "unresolved insider footnote reference",
            )?;
            if let Some(old) = local.insert(&row.line_id, row) {
                let mut comparable = old.clone();
                comparable.source_byte_start = row.source_byte_start;
                comparable.source_byte_end = row.source_byte_end;
                require(&comparable == row, "conflicting duplicate insider line")?;
                continue;
            }
            if filing.form.is_amendment() {
                let Some(target) = &row.supersedes else {
                    unresolved_amendment = true;
                    continue;
                };
                let Some((original_filing, _)) = active.get(target) else {
                    unresolved_amendment = true;
                    continue;
                };
                require(
                    filing.amendment_of.as_ref() == Some(&target.accession)
                        && original_filing.available_at < filing.available_at,
                    "amendment does not identify an earlier applicable filing line",
                )?;
                active.remove(target);
            } else {
                require(
                    row.supersedes.is_none() && filing.amendment_of.is_none(),
                    "unmarked ownership amendment",
                )?;
            }
            active.insert(
                InsiderLineRef {
                    accession: filing.accession.clone(),
                    line_id: row.line_id.clone(),
                },
                (filing, row),
            );
        }
        if filing.form.is_amendment() && filing.transactions.is_empty() {
            // Holding-only amendments cannot silently certify a transaction subtotal.
            unresolved_amendment = true;
        }
    }

    let mut rows = Vec::new();
    let mut economic_rows: BTreeMap<&str, (&OwnershipFiling, &InsiderTransaction)> =
        BTreeMap::new();
    let mut purchased = Decimal::ZERO;
    let mut sold = Decimal::ZERO;
    let mut observed_cash_rows = 0_u32;
    let mut buyers = BTreeSet::new();
    let mut roles = BTreeSet::new();
    for (line, (filing, row)) in active {
        if filing.available_at < input.window_start
            || row.transaction_date < input.window_start.date_naive()
            || row.transaction_date > input.window_end.date_naive()
        {
            continue;
        }
        let kind = classify(row);
        let mut reasons = Vec::new();
        let duplicate = if let Some((_, prior)) = economic_rows.get(row.economic_id.as_str()) {
            require(
                same_economic_transaction(prior, row),
                "economic identity collides with a different transaction",
            )?;
            reasons.push("duplicate_economic_transaction".into());
            true
        } else {
            economic_rows.insert(&row.economic_id, (filing, row));
            false
        };
        let notional = match (row.shares, row.price_per_unit) {
            (Some(shares), Some(price)) => Some(mul(shares, price)?),
            _ => {
                reasons.push("missing_as_filed_quantity_or_price".into());
                None
            }
        };
        if row.transaction_currency.as_ref() != Some(&input.reporting_currency) {
            reasons.push("missing_or_different_currency_no_fx_inference".into());
        }
        if row.quantity_unit != "shares" && row.table == OwnershipTable::NonDerivative {
            reasons.push("non_derivative_quantity_is_not_in_share_units".into());
        }
        if let Some(adjustment) = &row.split_adjustment {
            require(
                adjustment.quantity_factor > Decimal::ZERO
                    && adjustment.price_factor > Decimal::ZERO
                    && mul(adjustment.quantity_factor, adjustment.price_factor)? == Decimal::ONE
                    && !adjustment.source_ref.is_empty(),
                "split factors must preserve as-filed notional",
            )?;
            if adjustment.available_at > input.window_end
                || adjustment.basis_date > input.window_end.date_naive()
            {
                reasons.push("future_split_adjustment_ignored_as_filed_notional_retained".into());
            }
        }
        let cash = matches!(
            kind,
            InsiderTransactionKind::ReviewedCashPurchase | InsiderTransactionKind::ReviewedCashSale
        );
        if !cash {
            reasons.push("not_a_reviewed_discretionary_looking_cash_transaction".into());
        }
        let included = !duplicate
            && cash
            && row.quantity_unit == "shares"
            && notional.is_some()
            && row.transaction_currency.as_ref() == Some(&input.reporting_currency);
        if included {
            let value =
                notional.ok_or_else(|| Error("missing included insider notional".into()))?;
            observed_cash_rows = observed_cash_rows
                .checked_add(1)
                .ok_or_else(|| Error("too many insider rows".into()))?;
            if kind == InsiderTransactionKind::ReviewedCashPurchase {
                purchased = add(purchased, value)?;
                if value >= input.minimum_cluster_value && row.owner_ids.len() == 1 {
                    buyers.insert(row.owner_ids[0].clone());
                    if let Some(owner) = filing
                        .owners
                        .iter()
                        .find(|o| o.owner_id == row.owner_ids[0])
                    {
                        if owner.is_director == Some(true) {
                            roles.insert("director".to_string());
                        }
                        if owner.is_officer == Some(true) {
                            roles.insert("officer".to_string());
                        }
                        if owner.is_ten_percent_owner == Some(true) {
                            roles.insert("ten_percent_owner".to_string());
                        }
                    }
                } else if row.owner_ids.len() > 1 {
                    reasons.push("joint_reporting_owners_not_counted_as_distinct_buyers".into());
                } else {
                    reasons.push("below_predeclared_cluster_value_floor".into());
                }
            } else {
                sold = add(sold, value)?;
            }
        }
        rows.push(InsiderRowAssessment {
            line,
            economic_id: row.economic_id.clone(),
            owner_ids: row.owner_ids.clone(),
            available_at: filing.available_at,
            transaction_date: row.transaction_date,
            kind,
            as_filed_notional: notional,
            notional_currency: row.transaction_currency.clone(),
            quantity_unit: row.quantity_unit.clone(),
            plan_annotation: row.plan_annotation,
            included_in_totals: included && !unresolved_amendment,
            reasons,
            source_ref: row.source_ref.clone(),
        });
    }
    let mut reasons = vec![
        "observed_public_filings_only_not_personal_wealth_or_independent_news_corroboration".into(),
    ];
    if let Some(reason) = &input.coverage_reason {
        reasons.push(reason.clone());
    }
    if input.coverage != FilingCoverage::CompleteForDeclaredScope {
        reasons.push("incomplete_coverage_does_not_establish_zero_activity".into());
    }
    if unresolved_amendment {
        reasons.push("unresolved_amendment_lineage_totals_withheld".into());
    }
    if observed_cash_rows == 0 {
        reasons.push("no_qualifying_cash_observations".into());
    }
    let totals_available = !unresolved_amendment && observed_cash_rows > 0;
    let distinct =
        u32::try_from(buyers.len()).map_err(|_| Error("too many reporting owners".into()))?;
    Ok(InsiderAssessment {
        issuer_cik: input.issuer_cik.clone(),
        as_of: at,
        version: "insider-publication-v1".into(),
        reporting_currency: input.reporting_currency.clone(),
        coverage: input.coverage,
        observed_purchase_value: totals_available.then_some(purchased),
        observed_sale_value: totals_available.then_some(sold),
        observed_net_value: if totals_available {
            Some(sub(purchased, sold)?)
        } else {
            None
        },
        distinct_qualifying_buyers: totals_available.then_some(distinct),
        purchase_cluster: totals_available.then_some(distinct >= input.minimum_distinct_buyers),
        role_breadth: if totals_available {
            roles.into_iter().collect()
        } else {
            Vec::new()
        },
        rows,
        reasons,
    })
}

fn classify(row: &InsiderTransaction) -> InsiderTransactionKind {
    use InsiderTransactionKind as K;
    if row.table == OwnershipTable::Derivative {
        return K::Derivative;
    }
    match row.transaction_code.as_str() {
        "M" => K::OptionExercise,
        "F" => K::TaxWithholding,
        "A" => K::Award,
        "G" => K::Gift,
        "P" | "S" => {
            if row.plan_annotation == PlanAnnotation::DisclosedPlan {
                return K::PlanDescribed;
            }
            let reviewed = row.cash_market_confirmed == Some(true)
                && row.plan_annotation == PlanAnnotation::NoPlanDisclosedAfterReview
                && row.review_reference.as_ref().is_some_and(|s| !s.is_empty());
            match (
                row.transaction_code.as_str(),
                row.acquired_or_disposed.as_deref(),
                reviewed,
            ) {
                ("P", Some("A"), true) => K::ReviewedCashPurchase,
                ("S", Some("D"), true) => K::ReviewedCashSale,
                _ => K::UncertainPurchaseOrSale,
            }
        }
        _ => K::Unknown,
    }
}

fn same_economic_transaction(a: &InsiderTransaction, b: &InsiderTransaction) -> bool {
    a.owner_ids == b.owner_ids
        && a.table == b.table
        && a.security_title == b.security_title
        && a.transaction_date == b.transaction_date
        && a.transaction_code == b.transaction_code
        && a.acquired_or_disposed == b.acquired_or_disposed
        && a.shares == b.shares
        && a.price_per_unit == b.price_per_unit
        && a.transaction_currency == b.transaction_currency
        && a.direct_or_indirect == b.direct_or_indirect
        && a.plan_annotation == b.plan_annotation
        && a.cash_market_confirmed == b.cash_market_confirmed
}

fn add(a: Decimal, b: Decimal) -> Result<Decimal> {
    a.checked_add(b)
        .ok_or_else(|| Error("insider addition overflow".into()))
}
fn sub(a: Decimal, b: Decimal) -> Result<Decimal> {
    a.checked_sub(b)
        .ok_or_else(|| Error("insider subtraction overflow".into()))
}
fn mul(a: Decimal, b: Decimal) -> Result<Decimal> {
    a.checked_mul(b)
        .ok_or_else(|| Error("insider multiplication overflow".into()))
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
    fn row(owner: &str, code: &str, id: &str) -> InsiderTransaction {
        InsiderTransaction {
            line_id: id.into(),
            economic_id: id.into(),
            owner_ids: vec![owner.into()],
            table: OwnershipTable::NonDerivative,
            security_title: "Common".into(),
            transaction_date: time().date_naive(),
            transaction_code: code.into(),
            acquired_or_disposed: Some(if code == "S" { "D" } else { "A" }.into()),
            shares: Some(Decimal::from(10)),
            quantity_unit: "shares".into(),
            price_per_unit: Some(Decimal::from(20)),
            transaction_currency: Some("USD".into()),
            direct_or_indirect: Some("D".into()),
            nature_of_ownership: None,
            shares_owned_following: None,
            conversion_or_exercise_price: None,
            exercise_date: None,
            expiration_date: None,
            underlying_security: None,
            underlying_shares: None,
            footnote_ids: vec![],
            plan_annotation: PlanAnnotation::NoPlanDisclosedAfterReview,
            cash_market_confirmed: Some(true),
            review_reference: Some("review:footnotes".into()),
            split_adjustment: None,
            supersedes: None,
            source_ref: "sec:fixture#line".into(),
            source_byte_start: 1,
            source_byte_end: 10,
            missing_reasons: vec![],
        }
    }
    fn filing(owner: &str, accession: &str, rows: Vec<InsiderTransaction>) -> OwnershipFiling {
        OwnershipFiling {
            issuer_cik: "0000000042".into(),
            issuer_name_as_filed: None,
            issuer_trading_symbol: None,
            accession: accession.into(),
            form: OwnershipForm::Form4,
            filed_at: time(),
            available_at: time(),
            period_of_report: time().date_naive(),
            original_submission_date: None,
            amendment_of: None,
            plan_checkbox: None,
            owners: vec![ReportingOwner {
                owner_id: owner.into(),
                name_as_filed: None,
                is_director: Some(true),
                is_officer: None,
                is_ten_percent_owner: None,
                is_other: None,
                officer_title: None,
                other_relationship: None,
                missing_reasons: vec![],
            }],
            transactions: rows,
            holdings: vec![],
            footnotes: vec![],
            remarks: None,
            source_ref: "sec:fixture".into(),
            source_content_hash: "hash".into(),
            missing_reasons: vec![],
        }
    }
    fn input(filings: Vec<OwnershipFiling>) -> InsiderInput {
        InsiderInput {
            issuer_cik: "0000000042".into(),
            window_start: time() - Duration::days(3),
            window_end: time(),
            reporting_currency: "USD".into(),
            minimum_cluster_value: Decimal::from(100),
            minimum_distinct_buyers: 2,
            coverage: FilingCoverage::Partial,
            coverage_reason: Some("exempt_insiders_not_covered".into()),
            filings,
        }
    }

    #[test]
    fn insider_codes_plans_and_unknowns_do_not_become_cash_purchases() {
        let mut rows = vec![
            row("a", "P", "p"),
            row("a", "S", "s"),
            row("a", "M", "m"),
            row("a", "F", "f"),
        ];
        let mut plan = row("a", "P", "plan");
        plan.plan_annotation = PlanAnnotation::DisclosedPlan;
        rows.push(plan);
        let mut unknown = row("a", "P", "unknown");
        unknown.plan_annotation = PlanAnnotation::Unknown;
        rows.push(unknown);
        let mut derivative = row("a", "P", "deriv");
        derivative.table = OwnershipTable::Derivative;
        rows.push(derivative);
        let result = assess(&input(vec![filing("a", "f1", rows)]), time()).unwrap();
        assert_eq!(result.observed_purchase_value, Some(Decimal::from(200)));
        assert_eq!(result.observed_sale_value, Some(Decimal::from(200)));
        assert_eq!(result.observed_net_value, Some(Decimal::ZERO));
        assert_eq!(result.distinct_qualifying_buyers, Some(1));
        assert_eq!(
            result.rows.iter().filter(|r| r.included_in_totals).count(),
            2
        );
    }

    #[test]
    fn insider_clusters_count_people_and_deduplicate_economic_rows() {
        let mut duplicate = row("a", "P", "a1");
        duplicate.line_id = "duplicate_xml_line".into();
        let a = filing(
            "a",
            "f1",
            vec![row("a", "P", "a1"), duplicate, row("a", "P", "a2")],
        );
        let b = filing("b", "f2", vec![row("b", "P", "b1")]);
        let result = assess(&input(vec![a.clone(), a, b]), time()).unwrap();
        assert_eq!(result.observed_purchase_value, Some(Decimal::from(600)));
        assert_eq!(result.distinct_qualifying_buyers, Some(2));
        assert_eq!(result.purchase_cluster, Some(true));
    }

    #[test]
    fn insider_amendments_replace_only_linked_lines_after_availability() {
        let original = filing("a", "f1", vec![row("a", "P", "p1"), row("a", "P", "p2")]);
        let mut revised = row("a", "P", "revised");
        revised.shares = Some(Decimal::from(5));
        revised.supersedes = Some(InsiderLineRef {
            accession: "f1".into(),
            line_id: "p1".into(),
        });
        let mut amendment = filing("a", "f2", vec![revised]);
        amendment.form = OwnershipForm::Form4Amendment;
        amendment.amendment_of = Some("f1".into());
        amendment.filed_at += Duration::hours(1);
        amendment.available_at = amendment.filed_at;
        let mut i = input(vec![original, amendment]);
        assert_eq!(
            assess(&i, time()).unwrap().observed_purchase_value,
            Some(Decimal::from(400))
        );
        i.window_end += Duration::hours(1);
        assert_eq!(
            assess(&i, i.window_end).unwrap().observed_purchase_value,
            Some(Decimal::from(300))
        );
    }

    #[test]
    fn insider_backtests_cannot_use_trade_date_before_filing() {
        let mut f = filing("a", "f1", vec![row("a", "P", "p")]);
        f.transactions[0].transaction_date -= Duration::days(2);
        f.available_at += Duration::days(1);
        let result = assess(&input(vec![f]), time()).unwrap();
        assert!(result.rows.is_empty());
        assert!(result.observed_purchase_value.is_none());
    }

    #[test]
    fn insider_unresolved_amendment_withholds_totals_and_overflow_rejects() {
        let original = filing("a", "f1", vec![row("a", "P", "p")]);
        let mut amendment = filing("a", "f2", vec![row("a", "P", "new")]);
        amendment.form = OwnershipForm::Form4Amendment;
        let result = assess(&input(vec![original.clone(), amendment]), time()).unwrap();
        assert!(result.observed_purchase_value.is_none());
        let mut huge = original;
        huge.transactions[0].shares = Some(Decimal::MAX);
        assert!(assess(&input(vec![huge]), time()).is_err());
    }

    #[test]
    fn insider_joint_owners_do_not_manufacture_a_distinct_person_cluster() {
        let mut f = filing("a", "f1", vec![row("a", "P", "p")]);
        let mut second = f.owners[0].clone();
        second.owner_id = "b".into();
        f.owners.push(second);
        f.transactions[0].owner_ids.push("b".into());
        let a = assess(&input(vec![f.clone()]), time()).unwrap();
        assert_eq!(a.observed_purchase_value, Some(Decimal::from(200)));
        assert_eq!(a.distinct_qualifying_buyers, Some(0));
        assert_eq!(a.purchase_cluster, Some(false));
        f.transactions[0].quantity_unit = "USD".into();
        assert!(
            assess(&input(vec![f]), time())
                .unwrap()
                .observed_purchase_value
                .is_none()
        );
    }

    #[test]
    fn insider_repeated_raw_content_at_different_offsets_is_one_line() {
        let original = row("a", "P", "same-content-hash");
        let mut copy = original.clone();
        copy.source_byte_start = 20;
        copy.source_byte_end = 30;
        let a = assess(
            &input(vec![filing("a", "f1", vec![original, copy])]),
            time(),
        )
        .unwrap();
        assert_eq!(a.rows.len(), 1);
        assert_eq!(a.observed_purchase_value, Some(Decimal::from(200)));
    }

    #[test]
    fn issuer_events_fixture_insider_has_strict_research_serialization() {
        let i = input(vec![filing(
            "synthetic-owner",
            "synthetic-accession",
            vec![row("synthetic-owner", "P", "synthetic-line")],
        )]);
        let mut json = serde_json::to_value(&i).unwrap();
        let decoded: InsiderInput = serde_json::from_value(json.clone()).unwrap();
        let assessment = assess(&decoded, time()).unwrap();
        assert_eq!(assessment.observed_purchase_value, Some(Decimal::from(200)));
        json.as_object_mut()
            .unwrap()
            .insert("order_authority".into(), serde_json::json!(true));
        assert!(serde_json::from_value::<InsiderInput>(json).is_err());
        let mut result = serde_json::to_value(&assessment).unwrap();
        assert_eq!(
            serde_json::from_value::<InsiderAssessment>(result.clone()).unwrap(),
            assessment
        );
        result
            .as_object_mut()
            .unwrap()
            .insert("confidence_increase".into(), serde_json::json!(10));
        assert!(serde_json::from_value::<InsiderAssessment>(result).is_err());
        if std::env::var("TRADY_EMIT_ISSUER_EVENTS_FIXTURES").as_deref() == Ok("1") {
            println!(
                "PUBLIC_INPUT_FIXTURE insider {}",
                serde_json::to_string(&i).unwrap()
            );
        }
    }
}

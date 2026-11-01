//! Offline evidence-linked stock research. Supplied references are not runtime attestations.

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use trady_domain::{Error, Result, require};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReportInput {
    pub schema_version: u32,
    pub horizon_months: u8,
    pub candidates: Vec<Candidate>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Candidate {
    pub symbol: String,
    pub company: String,
    pub region: Region,
    pub sector: String,
    pub currency: String,
    pub financials: Financials,
    pub quote: Option<ResearchQuote>,
    pub trailing_eps: Option<TrailingEps>,
    pub thesis: String,
    pub catalysts: Vec<String>,
    pub risks: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub enum Region {
    US,
    EU,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Financials {
    pub period_start: NaiveDate,
    pub period_end: NaiveDate,
    pub published_at: DateTime<Utc>,
    pub source_url: String,
    pub revenue_millions: Decimal,
    pub prior_year_revenue_millions: Option<Decimal>,
    pub operating_income_millions: Option<Decimal>,
    /// Must cover exactly the same period as revenue, in the same currency and units.
    pub free_cash_flow_millions: Option<Decimal>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResearchQuote {
    pub price: Decimal,
    pub currency: String,
    pub observed_at: DateTime<Utc>,
    pub source_url: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TrailingEps {
    pub diluted_eps: Decimal,
    pub currency: String,
    pub period_start: NaiveDate,
    pub period_end: NaiveDate,
    pub published_at: DateTime<Utc>,
    pub source_url: String,
}

fn text(value: &str, bound: usize) -> bool {
    !value.trim().is_empty() && value.len() <= bound && !value.chars().any(char::is_control)
}
fn evidence_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let host = rest.split('/').next().unwrap_or_default();
    text(url, 2048)
        && !url.chars().any(char::is_whitespace)
        && !url.contains(['@', '?', '#'])
        && host.contains('.')
        && !host.contains(':')
}
fn bounded_number(n: Decimal) -> bool {
    n.abs() <= Decimal::from(1_000_000_000_000_i64)
}
fn ratio(numerator: Decimal, denominator: Decimal) -> Result<Decimal> {
    numerator
        .checked_div(denominator)
        .and_then(|n| n.checked_mul(Decimal::from(100)))
        .map(|n| n.round_dp(4).normalize())
        .ok_or_else(|| Error("stock report arithmetic unavailable".into()))
}

pub fn generate(input: &ReportInput, as_of: DateTime<Utc>) -> Result<Value> {
    require(
        input.schema_version == 1 && (6..=12).contains(&input.horizon_months),
        "unsupported stock report version or horizon",
    )?;
    require(
        !input.candidates.is_empty() && input.candidates.len() <= 100,
        "stock report requires 1..100 candidates",
    )?;
    let mut identities = BTreeSet::new();
    let mut sectors = std::collections::BTreeMap::<&str, usize>::new();
    let mut reports = vec![];
    for candidate in &input.candidates {
        require(
            text(&candidate.symbol, 32)
                && candidate
                    .symbol
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b':'))
                && identities.insert(candidate.symbol.to_uppercase()),
            "invalid or duplicate stock report symbol",
        )?;
        require(
            text(&candidate.company, 160)
                && text(&candidate.sector, 80)
                && candidate.currency.len() == 3
                && candidate.currency.bytes().all(|b| b.is_ascii_uppercase())
                && text(&candidate.thesis, 2000)
                && !candidate.risks.is_empty()
                && candidate.risks.len() <= 20
                && candidate.catalysts.len() <= 20
                && candidate
                    .risks
                    .iter()
                    .chain(&candidate.catalysts)
                    .all(|s| text(s, 1000)),
            "invalid stock report narrative or currency",
        )?;
        let f = &candidate.financials;
        require(
            (1..=380).contains(&(f.period_end - f.period_start).num_days())
                && f.period_end <= f.published_at.date_naive()
                && f.published_at <= as_of
                && evidence_url(&f.source_url),
            "invalid or future stock report financial evidence",
        )?;
        require(
            f.revenue_millions > Decimal::ZERO
                && bounded_number(f.revenue_millions)
                && f.prior_year_revenue_millions
                    .is_none_or(|n| n > Decimal::ZERO && bounded_number(n))
                && [f.operating_income_millions, f.free_cash_flow_millions]
                    .into_iter()
                    .flatten()
                    .all(bounded_number),
            "invalid stock report financial amount",
        )?;
        let revenue_growth_pct = f
            .prior_year_revenue_millions
            .map(|prior| ratio(f.revenue_millions - prior, prior))
            .transpose()?;
        let operating_margin_pct = f
            .operating_income_millions
            .map(|n| ratio(n, f.revenue_millions))
            .transpose()?;
        let free_cash_flow_margin_pct = f
            .free_cash_flow_millions
            .map(|n| ratio(n, f.revenue_millions))
            .transpose()?;
        let mut gaps = vec![];
        let publication_age_days = (as_of - f.published_at).num_days();
        let period_age_days = (as_of.date_naive() - f.period_end).num_days();
        let stale = publication_age_days > 120 || period_age_days > 180;
        if stale {
            gaps.push("financial_evidence_stale");
        }
        if f.prior_year_revenue_millions.is_none() {
            gaps.push("prior_year_revenue_unavailable");
        }
        if f.operating_income_millions.is_none() {
            gaps.push("operating_income_unavailable");
        }
        if f.free_cash_flow_millions.is_none() {
            gaps.push("same_period_free_cash_flow_unavailable");
        }
        let caution = revenue_growth_pct.is_some_and(|n| n < Decimal::ZERO)
            || f.operating_income_millions
                .is_some_and(|n| n <= Decimal::ZERO)
            || f.free_cash_flow_millions
                .is_some_and(|n| n <= Decimal::ZERO);
        let fundamentals = if stale {
            "STALE_EVIDENCE"
        } else if caution {
            "CAUTION"
        } else if revenue_growth_pct.is_some()
            && operating_margin_pct.is_some()
            && free_cash_flow_margin_pct.is_some()
        {
            "SUPPORTED"
        } else {
            "INCOMPLETE"
        };
        let mut quote_current = false;
        if let Some(quote) = &candidate.quote {
            require(
                quote.price > Decimal::ZERO
                    && bounded_number(quote.price)
                    && quote.currency == candidate.currency
                    && quote.observed_at <= as_of
                    && evidence_url(&quote.source_url),
                "invalid, future or mismatched stock report quote",
            )?;
            quote_current = (as_of - quote.observed_at).num_seconds() <= 4 * 86400;
            if !quote_current {
                gaps.push("quote_stale");
            }
        } else {
            gaps.push("quote_unavailable");
        }
        let mut eps_current = false;
        if let Some(eps) = &candidate.trailing_eps {
            let duration = (eps.period_end - eps.period_start).num_days();
            require(
                (350..=380).contains(&duration)
                    && eps.period_end <= eps.published_at.date_naive()
                    && eps.published_at <= as_of
                    && eps.currency == candidate.currency
                    && bounded_number(eps.diluted_eps)
                    && evidence_url(&eps.source_url),
                "invalid stock report trailing EPS evidence",
            )?;
            eps_current = eps.diluted_eps > Decimal::ZERO
                && (as_of.date_naive() - eps.period_end).num_days() <= 180
                && (as_of - eps.published_at).num_days() <= 120;
            if !eps_current {
                gaps.push("trailing_eps_stale_or_nonpositive");
            }
        } else {
            gaps.push("trailing_eps_unavailable");
        }
        let pe_ttm = if quote_current && eps_current {
            candidate
                .quote
                .as_ref()
                .unwrap()
                .price
                .checked_div(candidate.trailing_eps.as_ref().unwrap().diluted_eps)
                .map(|n| n.round_dp(4).normalize())
                .ok_or_else(|| Error("stock report valuation overflow".into()))?
                .into()
        } else {
            None::<Decimal>
        };
        *sectors.entry(&candidate.sector).or_default() += 1;
        reports.push(json!({
            "symbol":candidate.symbol,"company":candidate.company,"region":candidate.region,"sector":candidate.sector,
            "currency":candidate.currency,"financial_period_start":f.period_start,"financial_period_end":f.period_end,
            "source_url":f.source_url,"published_at":f.published_at,"publication_age_days":publication_age_days,
            "fundamentals":fundamentals,"revenue_growth_pct":revenue_growth_pct,"operating_margin_pct":operating_margin_pct,
            "free_cash_flow_margin_pct":free_cash_flow_margin_pct,"quote":candidate.quote,"pe_ttm":pe_ttm,
            "trailing_eps_evidence":candidate.trailing_eps,"valuation_review_required":true,
            "evidence_gaps":gaps,"thesis":candidate.thesis,"catalysts":candidate.catalysts,"risks":candidate.risks,
            "entry_authorized":false
        }));
    }
    Ok(
        json!({"schema_version":1,"as_of":as_of,"market":"US_EU","risk_profile":"MODERATE",
        "horizon_months":input.horizon_months,"purpose":"RESEARCH_ONLY","source_verification":"SUPPLIED_REFERENCES",
        "sector_counts":sectors,"candidates":reports,"entry_authorized":false}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (ReportInput, DateTime<Utc>) {
        let now = DateTime::parse_from_rfc3339("2026-10-31T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let input=serde_json::from_value(json!({"schema_version":1,"horizon_months":12,"candidates":[{
            "symbol":"EXAMPLE","company":"Example","region":"US","sector":"Software","currency":"USD",
            "financials":{"period_start":"2026-07-01","period_end":"2026-09-30","published_at":"2026-10-20T00:00:00Z",
                "source_url":"https://example.com/results","revenue_millions":"120","prior_year_revenue_millions":"100",
                "operating_income_millions":"30","free_cash_flow_millions":"24"},"quote":null,"trailing_eps":null,
            "thesis":"Recurring revenue","catalysts":["Next results"],"risks":["Demand slowdown"]}]})).unwrap();
        (input, now)
    }
    #[test]
    fn calculates_period_matched_ratios_and_keeps_missing_valuation_visible() {
        let (input, now) = fixture();
        let report = generate(&input, now).unwrap();
        let row = &report["candidates"][0];
        assert_eq!(row["revenue_growth_pct"], "20");
        assert_eq!(row["operating_margin_pct"], "25");
        assert_eq!(row["free_cash_flow_margin_pct"], "20");
        assert_eq!(row["fundamentals"], "SUPPORTED");
        assert!(row["pe_ttm"].is_null());
        assert_eq!(row["entry_authorized"], false);
        assert_eq!(report["entry_authorized"], false);
    }
    #[test]
    fn rejects_future_zero_baseline_and_duplicate_evidence() {
        let (mut input, now) = fixture();
        input.candidates[0].financials.published_at = now + chrono::Duration::days(1);
        assert!(generate(&input, now).is_err());
        let (mut input, now) = fixture();
        input.candidates[0].financials.prior_year_revenue_millions = Some(Decimal::ZERO);
        assert!(generate(&input, now).is_err());
        let (mut input, now) = fixture();
        input.candidates.push(input.candidates[0].clone());
        assert!(generate(&input, now).is_err());
    }
    #[test]
    fn stale_and_negative_cash_flow_are_never_supported() {
        let (mut input, now) = fixture();
        input.candidates[0].financials.free_cash_flow_millions = Some(-Decimal::ONE);
        assert_eq!(
            generate(&input, now).unwrap()["candidates"][0]["fundamentals"],
            "CAUTION"
        );
        let (input, now) = fixture();
        assert_eq!(
            generate(&input, now + chrono::Duration::days(181)).unwrap()["candidates"][0]["fundamentals"],
            "STALE_EVIDENCE"
        );
    }
    #[test]
    fn valuation_requires_current_quote_same_currency_and_full_year_eps() {
        let (mut input, now) = fixture();
        input.candidates[0].quote = Some(ResearchQuote {
            price: Decimal::from(60),
            currency: "USD".into(),
            observed_at: now,
            source_url: "https://example.com/quote".into(),
        });
        input.candidates[0].trailing_eps = Some(TrailingEps {
            diluted_eps: Decimal::from(3),
            currency: "USD".into(),
            period_start: NaiveDate::from_ymd_opt(2025, 10, 1).unwrap(),
            period_end: NaiveDate::from_ymd_opt(2026, 9, 30).unwrap(),
            published_at: now,
            source_url: "https://example.com/annual".into(),
        });
        assert_eq!(
            generate(&input, now).unwrap()["candidates"][0]["pe_ttm"],
            "20"
        );
        input.candidates[0].quote.as_mut().unwrap().observed_at = now - chrono::Duration::days(5);
        assert!(generate(&input, now).unwrap()["candidates"][0]["pe_ttm"].is_null());
        input.candidates[0].quote.as_mut().unwrap().currency = "EUR".into();
        assert!(generate(&input, now).is_err());
        input.candidates[0].quote.as_mut().unwrap().currency = "USD".into();
        input.candidates[0]
            .trailing_eps
            .as_mut()
            .unwrap()
            .period_start = NaiveDate::from_ymd_opt(2026, 7, 1).unwrap();
        assert!(generate(&input, now).is_err());
    }
}

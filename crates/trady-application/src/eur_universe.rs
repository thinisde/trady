//! Deterministic broker-catalog reconciliation. Provider symbols never become
//! order identities without an exact reviewed catalog ticker and ISIN match.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use trady_domain::{Currency, EurInstrumentBinding, EurInstrumentRegistryEntry, Result, require};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct CatalogIdentity {
    pub broker_ticker: String,
    pub isin: String,
    pub currency: String,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewedInstrument {
    pub binding: EurInstrumentBinding,
    pub alpha_vantage_symbol: String,
    pub owner_review_reference: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogRejection {
    pub broker_ticker: String,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistrySnapshot {
    pub schema_version: u32,
    pub catalog_hash: String,
    pub registry_hash: String,
    pub members: Vec<EurInstrumentRegistryEntry>,
    pub rejections: Vec<CatalogRejection>,
}

pub fn build_registry(
    catalog: &[CatalogIdentity],
    reviewed: &[ReviewedInstrument],
    now: DateTime<Utc>,
) -> Result<RegistrySnapshot> {
    require(
        catalog.len() <= 100_000 && reviewed.len() <= 2048,
        "EUR catalog or review exceeds its bound",
    )?;
    let mut catalog = catalog.to_vec();
    catalog.sort();
    let mut tickers = BTreeSet::new();
    require(
        catalog
            .iter()
            .all(|row| tickers.insert(row.broker_ticker.clone())),
        "broker catalog contains ambiguous ticker identities",
    )?;
    let mut mappings = BTreeMap::new();
    for review in reviewed {
        review.binding.validate(now)?;
        require(
            !review.owner_review_reference.is_empty()
                && !review
                    .owner_review_reference
                    .to_ascii_lowercase()
                    .contains("configured")
                && mappings
                    .insert(review.binding.broker_instrument_id.clone(), review)
                    .is_none(),
            "reviewed registry has a missing approval or duplicate broker identity",
        )?;
    }
    let mut members = Vec::new();
    let mut rejections = Vec::new();
    let mut ids = BTreeSet::new();
    let mut isins = BTreeSet::new();
    for row in &catalog {
        let mut reasons = Vec::new();
        if row.currency != "EUR" {
            reasons.push("NON_EUR".into());
        }
        if row.kind != "STOCK" {
            reasons.push("NOT_EQUITY".into());
        }
        if let Some(review) = mappings.get(&row.broker_ticker) {
            if review.binding.isin != row.isin {
                reasons.push("ISIN_CHANGED".into());
            }
            if reasons.is_empty() {
                let mut entry = EurInstrumentRegistryEntry::from_binding(
                    &review.binding,
                    review.alpha_vantage_symbol.clone(),
                );
                entry.owner_review_reference = review.owner_review_reference.clone();
                entry.validate(now)?;
                require(
                    ids.insert(entry.internal_instrument_id.clone())
                        && isins.insert(entry.isin.clone()),
                    "reviewed registry has an ambiguous internal ID or ISIN",
                )?;
                members.push(entry);
            }
        } else {
            reasons.push("NO_INDEPENDENT_CROSSWALK_REVIEW".into());
        }
        if !reasons.is_empty() {
            rejections.push(CatalogRejection {
                broker_ticker: row.broker_ticker.clone(),
                reasons,
            });
        }
    }
    for (ticker, _) in mappings {
        if !tickers.contains(&ticker) {
            rejections.push(CatalogRejection {
                broker_ticker: ticker,
                reasons: vec!["CATALOG_ID_CHANGED_OR_MISSING".into()],
            });
        }
    }
    let catalog_hash = hash(&catalog)?;
    let registry_hash = hash(&(&catalog_hash, &members, &rejections))?;
    Ok(RegistrySnapshot {
        schema_version: 1,
        catalog_hash,
        registry_hash,
        members,
        rejections,
    })
}

fn hash(value: &impl Serialize) -> Result<String> {
    serde_json::to_string(value)
        .map(|text| crate::harness::content_hash(&text))
        .map_err(|_| trady_domain::Error("EUR registry encoding failed".into()))
}

impl RegistrySnapshot {
    pub fn check_currency(&self) -> Result<()> {
        require(
            self.members
                .iter()
                .all(|m| m.currency == Currency::new("EUR").expect("constant")),
            "registry contains a non-EUR instrument",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_permutations_are_stable_and_catalog_or_isin_changes_invalidate_the_registry() {
        let at = DateTime::parse_from_rfc3339("2026-11-03T10:00:00Z")
            .unwrap()
            .to_utc();
        let universe: trady_domain::EurUniverseSnapshot = serde_json::from_str(include_str!(
            "../../../config/eur_equity_universe.example.json"
        ))
        .unwrap();
        let review = ReviewedInstrument {
            binding: universe.members[0].clone(),
            alpha_vantage_symbol: "reviewed_ALPHA_SYMBOL".into(),
            owner_review_reference: "independent_review_42".into(),
        };
        let rows = vec![
            CatalogIdentity {
                broker_ticker: review.binding.broker_instrument_id.clone(),
                isin: review.binding.isin.clone(),
                currency: "EUR".into(),
                kind: "STOCK".into(),
            },
            CatalogIdentity {
                broker_ticker: "USD_EQ".into(),
                isin: "US0378331005".into(),
                currency: "USD".into(),
                kind: "STOCK".into(),
            },
        ];
        let original = build_registry(&rows, std::slice::from_ref(&review), at).unwrap();
        let mut reversed = rows.clone();
        reversed.reverse();
        assert_eq!(
            original,
            build_registry(&reversed, std::slice::from_ref(&review), at).unwrap()
        );
        assert_eq!(original.members.len(), 1);
        assert!(
            original
                .rejections
                .iter()
                .any(|r| r.reasons.contains(&"NON_EUR".into()))
        );
        let mut changed = rows.clone();
        changed[0].isin = "NL0010273215".into();
        let invalid = build_registry(&changed, std::slice::from_ref(&review), at).unwrap();
        assert!(invalid.members.is_empty());
        assert_ne!(invalid.registry_hash, original.registry_hash);
        let mut renamed = rows;
        renamed[0].broker_ticker = "RENAMED_EQ".into();
        let invalid = build_registry(&renamed, std::slice::from_ref(&review), at).unwrap();
        assert!(invalid.members.is_empty());
        assert!(
            invalid
                .rejections
                .iter()
                .any(|r| r.reasons.contains(&"CATALOG_ID_CHANGED_OR_MISSING".into()))
        );
    }
}

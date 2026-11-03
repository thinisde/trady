//! Fail-closed integrity checks for the registered long Donchian authority.
//!
//! The authority JSON is intentionally kept separate from this manifest.  The
//! manifest pins the bytes used by a deployment, while `FormulaAuthority`
//! continues to describe the formula contract consumed by the calculator.  A
//! runtime can therefore verify provenance without making a hash field supplied
//! by an analyst part of the calculation input.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use trady_domain::{Error, Result, require};
use trady_policy::{Policy, limits::DonchianModelFusion};

use super::FormulaAuthority;

pub const LONG_DONCHIAN_FORMULA_ID: &str = "long_donchian_atr_v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DonchianReleaseState {
    Research,
    Shadow,
    PaperValidated,
    Approved,
    Paused,
    Retired,
}

impl DonchianReleaseState {
    fn permits_live(&self) -> bool {
        matches!(self, Self::Approved)
    }
}

/// Content-addressed deployment evidence for the deterministic authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DonchianIntegrityManifest {
    pub schema_version: u32,
    pub formula_id: String,
    pub authority_id: String,
    pub authority_sha256: String,
    pub spec_sha256: String,
    pub universe_sha256: String,
    pub runtime_sha256: String,
    pub implementation_sha256: String,
    pub status: DonchianReleaseState,
    pub live_enabled: bool,
    pub entries_paused: bool,
    /// Evidence IDs are deployment-owned references, not caller verdicts.
    pub evidence: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DonchianIntegrityReport {
    pub formula_id: String,
    pub authority_sha256: String,
    pub spec_sha256: String,
    pub universe_sha256: String,
    pub runtime_sha256: String,
    pub implementation_sha256: String,
    pub status: DonchianReleaseState,
    pub live_enabled: bool,
    pub entries_paused: bool,
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_-.:".contains(&byte))
}

/// Verify all bytes and status flags needed before a deterministic deployment
/// can be considered for live execution.
#[allow(clippy::too_many_arguments)]
pub fn verify_donchian_integrity(
    manifest: &DonchianIntegrityManifest,
    authority_bytes: &[u8],
    spec_bytes: &[u8],
    universe_bytes: &[u8],
    runtime_bytes: &[u8],
    implementation_bytes: &[u8],
    runtime_authority: &FormulaAuthority,
    policy: &Policy,
) -> Result<DonchianIntegrityReport> {
    require(
        manifest.schema_version == 1
            && manifest.formula_id == LONG_DONCHIAN_FORMULA_ID
            && manifest.authority_id == LONG_DONCHIAN_FORMULA_ID
            && valid_hash(&manifest.authority_sha256)
            && valid_hash(&manifest.spec_sha256)
            && valid_hash(&manifest.universe_sha256)
            && valid_hash(&manifest.runtime_sha256)
            && valid_hash(&manifest.implementation_sha256),
        "Donchian authority integrity manifest is malformed",
    )?;
    require(
        manifest.authority_sha256 == sha256(authority_bytes)
            && manifest.spec_sha256 == sha256(spec_bytes)
            && manifest.universe_sha256 == sha256(universe_bytes)
            && manifest.runtime_sha256 == sha256(runtime_bytes)
            && manifest.implementation_sha256 == sha256(implementation_bytes),
        "Donchian authority integrity hash mismatch",
    )?;
    require(
        manifest.live_enabled == manifest.status.permits_live()
            && manifest.entries_paused != manifest.status.permits_live(),
        "Donchian release state is not fail-closed",
    )?;
    for key in ["proof", "parity", "evaluation", "shadow", "owner"] {
        require(
            manifest
                .evidence
                .get(key)
                .is_some_and(|value| valid_id(value)),
            "Donchian release evidence is incomplete",
        )?;
    }

    let canonical: FormulaAuthority = serde_json::from_slice(authority_bytes)
        .map_err(|_| Error("canonical Donchian authority is unreadable".into()))?;
    let spec: serde_json::Value = serde_json::from_slice(spec_bytes)
        .map_err(|_| Error("canonical Donchian specification is unreadable".into()))?;
    require(
        spec["schema_version"] == 1
            && spec["formula_id"] == LONG_DONCHIAN_FORMULA_ID
            && spec["universe"] == "IEX30"
            && spec["direction"] == "LONG_ONLY"
            && spec["bar_interval"] == "1D_REGULAR_US_SESSION"
            && spec["breakout_window"] == 20
            && spec["atr_method"] == "WILDER"
            && spec["atr_window"] == 14
            && spec["minimum_warmup_bars"] == 35
            && spec["entry_allowance_atr"] == "0.10"
            && spec["stop_atr_multiple"] == "1.50"
            && spec["target_r_multiple"] == "1.50"
            && spec["maximum_hold_sessions"] == 5
            && spec["same_bar_rule"] == "STOP_FIRST"
            && spec["gap_stop_rule"] == "FIRST_EXECUTABLE_PRICE"
            && spec["trailing_stop"] == false
            && spec["averaging"] == false
            && spec["pyramiding"] == false
            && spec["leverage"] == false
            && spec["model_fusion"] == "DISABLED",
        "canonical Donchian specification diverges from the registered baseline",
    )?;
    let universe: serde_json::Value = serde_json::from_slice(universe_bytes)
        .map_err(|_| Error("IEX30 universe is unreadable".into()))?;
    let symbols = universe["symbols"]
        .as_array()
        .ok_or_else(|| Error("IEX30 universe symbols are missing".into()))?;
    let mut symbol_values = symbols
        .iter()
        .map(|value| value.as_str().unwrap_or_default())
        .collect::<Vec<_>>();
    symbol_values.sort_unstable();
    symbol_values.dedup();
    require(
        universe["universe_id"] == "IEX30"
            && universe["venue"] == "ALPACA_IEX"
            && universe["feed"] == "iex"
            && universe["currency"] == "USD"
            && symbols.len() == 30
            && symbol_values.len() == 30,
        "canonical IEX30 universe diverges from the registered baseline",
    )?;
    require(
        canonical == *runtime_authority,
        "runtime Donchian authority differs from the canonical authority",
    )?;
    canonical.validate(policy)?;
    let strategy = canonical
        .strategy
        .as_ref()
        .ok_or_else(|| Error("canonical Donchian strategy binding is missing".into()))?;
    let evidence = [
        ("proof", canonical.active.proof_report_id.as_str()),
        ("parity", canonical.active.parity_report_id.as_str()),
        ("evaluation", canonical.active.evaluation_report_id.as_str()),
        ("shadow", canonical.active.shadow_report_id.as_str()),
        ("owner", canonical.active.owner_approval_id.as_str()),
    ];
    require(
        evidence.iter().all(|(key, value)| {
            manifest
                .evidence
                .get(*key)
                .is_some_and(|actual| actual == value)
        }),
        "Donchian integrity evidence does not match the canonical authority",
    )?;
    require(
        strategy.formula_id == LONG_DONCHIAN_FORMULA_ID
            && matches!(strategy.model_fusion, DonchianModelFusion::Disabled)
            && canonical.prediction.is_none(),
        "Donchian authority enables an unsupported model or fusion path",
    )?;

    Ok(DonchianIntegrityReport {
        formula_id: manifest.formula_id.clone(),
        authority_sha256: manifest.authority_sha256.clone(),
        spec_sha256: manifest.spec_sha256.clone(),
        universe_sha256: manifest.universe_sha256.clone(),
        runtime_sha256: manifest.runtime_sha256.clone(),
        implementation_sha256: manifest.implementation_sha256.clone(),
        status: manifest.status,
        live_enabled: manifest.live_enabled,
        entries_paused: manifest.entries_paused,
    })
}

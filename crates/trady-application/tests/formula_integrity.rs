use std::fs;

use trady_application::optimization::{
    DonchianIntegrityManifest, DonchianReleaseState, verify_donchian_integrity,
};
use trady_policy::Policy;

struct Inputs {
    manifest: DonchianIntegrityManifest,
    authority: Vec<u8>,
    spec: Vec<u8>,
    universe: Vec<u8>,
    runtime: Vec<u8>,
    implementation: Vec<u8>,
    runtime_authority: trady_application::optimization::FormulaAuthority,
    policy: Policy,
}

fn inputs() -> Inputs {
    let config = fs::read("../../config/autonomous.iex30.demo.json").unwrap();
    let document: serde_json::Value = serde_json::from_slice(&config).unwrap();
    let authority: trady_application::optimization::FormulaAuthority =
        serde_json::from_value(document["trading"]["limits_authority"].clone()).unwrap();
    let mut manifest: DonchianIntegrityManifest = serde_json::from_slice(
        &fs::read("../../config/formulas/long-donchian-atr-v1.integrity.json").unwrap(),
    )
    .unwrap();
    let implementation = fs::read("../../crates/trady-policy/src/limits/donchian.rs").unwrap();
    // Synthetic positive fixture only. The checked-in historical authority is
    // deliberately not re-signed when executable behavior changes.
    manifest.implementation_sha256 =
        trady_application::harness::content_hash(std::str::from_utf8(&implementation).unwrap());
    Inputs {
        manifest,
        authority: fs::read("../../config/formulas/long-donchian-atr-v1.authority.json").unwrap(),
        spec: fs::read("../../config/formulas/long-donchian-atr-v1.json").unwrap(),
        universe: fs::read("../../config/iex30.universe.json").unwrap(),
        runtime: config,
        implementation,
        runtime_authority: authority,
        policy: serde_json::from_value(document["trading"]["policy"].clone()).unwrap(),
    }
}

#[test]
fn matching_synthetic_manifest_is_content_addressed_and_paused() {
    let Inputs {
        manifest,
        authority,
        spec,
        universe,
        runtime,
        implementation,
        runtime_authority,
        policy,
    } = inputs();
    let report = verify_donchian_integrity(
        &manifest,
        &authority,
        &spec,
        &universe,
        &runtime,
        &implementation,
        &runtime_authority,
        &policy,
    )
    .unwrap();
    assert_eq!(report.status, DonchianReleaseState::Shadow);
    assert!(!report.live_enabled);
    assert!(report.entries_paused);
}

#[test]
fn historical_registration_does_not_approve_the_corrected_exit_implementation() {
    let mut fixture = inputs();
    fixture.manifest = serde_json::from_slice(
        &fs::read("../../config/formulas/long-donchian-atr-v1.integrity.json").unwrap(),
    )
    .unwrap();
    let error = verify_donchian_integrity(
        &fixture.manifest,
        &fixture.authority,
        &fixture.spec,
        &fixture.universe,
        &fixture.runtime,
        &fixture.implementation,
        &fixture.runtime_authority,
        &fixture.policy,
    )
    .unwrap_err();
    assert_eq!(error.0, "Donchian authority integrity hash mismatch");
}

#[test]
fn runtime_byte_or_authority_drift_blocks_integrity_verification() {
    let Inputs {
        manifest,
        authority,
        spec,
        universe,
        mut runtime,
        implementation,
        runtime_authority,
        policy,
    } = inputs();
    runtime.push(b'\n');
    assert!(
        verify_donchian_integrity(
            &manifest,
            &authority,
            &spec,
            &universe,
            &runtime,
            &implementation,
            &runtime_authority,
            &policy,
        )
        .is_err()
    );
}

#[test]
fn universe_drift_blocks_integrity_verification() {
    let Inputs {
        manifest,
        authority,
        spec,
        mut universe,
        runtime,
        implementation,
        runtime_authority,
        policy,
    } = inputs();
    universe.push(b'\n');
    assert!(
        verify_donchian_integrity(
            &manifest,
            &authority,
            &spec,
            &universe,
            &runtime,
            &implementation,
            &runtime_authority,
            &policy,
        )
        .is_err()
    );
}

//! Runtime universe ingestion, screening and immutable snapshot persistence.
//!
//! The caller owns source approval, broker verification and observation gathering.
//! Neither these ports nor their results expose an order submission operation.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use trady_domain::{
    Error, InstrumentId, Result, require,
    screening::{
        MAX_UNIVERSE_ROWS, ScreeningObservations, ScreeningPolicy, ScreeningResult,
        UniverseSnapshot, screen_universe, valid_reference,
    },
};

pub const MAX_SNAPSHOT_BYTES: usize = 8 * 1024 * 1024;

pub trait UniverseSource {
    /// Fetch current permitted provider data at runtime. Never stamp old data as
    /// current or infer historical availability from a download timestamp.
    fn fetch_universe(&mut self, now: DateTime<Utc>) -> Result<UniverseSnapshot>;
}

pub trait ScreeningDataPort {
    /// Supply verified account/instrument support, current execution observations
    /// and evidence-backed priority calculations. Missing observations stay missing.
    fn observations(
        &mut self,
        universe: &UniverseSnapshot,
        now: DateTime<Utc>,
    ) -> Result<ScreeningObservations>;
}

pub trait ScreeningRepository {
    /// Persist the complete immutable snapshot before it can be used by a cycle.
    /// Implementations must atomically bind owner/run to content, deduplicate
    /// identical retries, reject changed retries, and enforce source retention.
    /// PostgreSQL remains the production authority; in-memory stores are fixtures.
    fn store_screening_snapshot(&mut self, snapshot: &ScreeningSnapshot) -> Result<()>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScreeningRequest {
    /// Trusted application scope, never values accepted from source documents.
    pub owner_id: String,
    pub run_id: String,
    /// Explicitly empty means the owner permits no new instruments.
    pub owner_allowlist: BTreeSet<InstrumentId>,
    pub policy: ScreeningPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ScreeningFailure {
    UniverseUnavailable,
    SourceStorageNotPermitted,
    ObservationsUnavailable,
    InvalidInputs,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScreeningSnapshot {
    /// Application-generated SHA-256 identity over the scoped immutable content.
    pub snapshot_id: String,
    pub schema_version: u32,
    pub as_of: DateTime<Utc>,
    pub request: ScreeningRequest,
    pub universe: Option<UniverseSnapshot>,
    pub observations: Option<ScreeningObservations>,
    pub result: Option<ScreeningResult>,
    /// Sanitized code only: no provider text, credentials or raw HTTP body.
    pub failure: Option<ScreeningFailure>,
}

/// Run a single bounded screening pass. Provider/data failures become persisted
/// failed attempts. Persistence errors propagate; an unpersisted result is never
/// returned as successful. A populated result is still research only.
pub fn run_screening(
    request: &ScreeningRequest,
    source: &mut impl UniverseSource,
    data: &mut impl ScreeningDataPort,
    repository: &mut impl ScreeningRepository,
    now: DateTime<Utc>,
) -> Result<ScreeningSnapshot> {
    require(
        valid_reference(&request.owner_id)
            && valid_reference(&request.run_id)
            && request.owner_allowlist.len() <= MAX_UNIVERSE_ROWS,
        "invalid screening scope",
    )?;
    request.policy.validate()?;
    let mut snapshot = ScreeningSnapshot {
        snapshot_id: String::new(),
        schema_version: 1,
        as_of: now,
        request: request.clone(),
        universe: None,
        observations: None,
        result: None,
        failure: None,
    };
    match source.fetch_universe(now) {
        Err(_) => snapshot.failure = Some(ScreeningFailure::UniverseUnavailable),
        Ok(universe) if !universe.provenance.rights.store_content => {
            // Do not persist the forbidden dataset or derivatives of its content.
            snapshot.failure = Some(ScreeningFailure::SourceStorageNotPermitted);
        }
        Ok(universe) => {
            if universe.members.len() > MAX_UNIVERSE_ROWS {
                snapshot.failure = Some(ScreeningFailure::InvalidInputs);
            } else {
                match data.observations(&universe, now) {
                    Err(_) => snapshot.failure = Some(ScreeningFailure::ObservationsUnavailable),
                    Ok(observations) => {
                        match screen_universe(
                            &universe,
                            &request.owner_allowlist,
                            &observations,
                            &request.policy,
                            now,
                        ) {
                            Ok(result) => snapshot.result = Some(result),
                            Err(_) => snapshot.failure = Some(ScreeningFailure::InvalidInputs),
                        }
                        snapshot.observations = Some(observations);
                    }
                }
                snapshot.universe = Some(universe);
            }
        }
    }
    let mut bytes = encode(&snapshot)?;
    if bytes.len() > MAX_SNAPSHOT_BYTES {
        snapshot.universe = None;
        snapshot.observations = None;
        snapshot.result = None;
        snapshot.failure = Some(ScreeningFailure::InvalidInputs);
        bytes = encode(&snapshot)?;
    }
    snapshot.snapshot_id = format!("screen_{}", hex_digest(&bytes));
    repository.store_screening_snapshot(&snapshot)?;
    Ok(snapshot)
}

fn encode(snapshot: &ScreeningSnapshot) -> Result<Vec<u8>> {
    serde_json::to_vec(snapshot).map_err(|_| Error("screening snapshot encoding failed".into()))
}

pub fn hex_digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

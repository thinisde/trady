//! A recording-only executor transport. It has no network client or broker handle.
//! Proposed mutations are durably recorded and return Unsupported: no order,
//! fill, cancellation acknowledgement or performance is invented.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};
use trady_application::{
    autonomous::{ExecutionBroker, ExecutionObservation, TrackedOrder},
    broker::*,
    optimization::{digest, valid_hash, valid_id},
};
use trady_domain::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum ShadowSource {
    Real { market_data_hash: String },
    Fixture { description: String },
}
pub struct ShadowBroker {
    observation: ExecutionObservation,
    source: ShadowSource,
    directory: PathBuf,
}
impl ShadowBroker {
    pub fn new(
        directory: &Path,
        observation: ExecutionObservation,
        source: ShadowSource,
    ) -> Result<Self> {
        validate_observation(&observation, &source)?;
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder
            .create(directory)
            .map_err(|_| Error("cannot create shadow recording directory".into()))?;
        let meta = fs::symlink_metadata(directory)
            .map_err(|_| Error("shadow directory unavailable".into()))?;
        require(
            meta.is_dir() && !meta.file_type().is_symlink(),
            "invalid shadow recording directory",
        )?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            require(
                meta.mode() & 0o077 == 0,
                "shadow recording directory must be private to its owner",
            )?;
        }
        Ok(Self {
            observation,
            source,
            directory: directory.to_owned(),
        })
    }
    pub fn update(
        &mut self,
        observation: ExecutionObservation,
        source: ShadowSource,
    ) -> Result<String> {
        validate_observation(&observation, &source)?;
        require(
            observation.snapshot.account_id == self.observation.snapshot.account_id
                && observation.snapshot.observed_at >= self.observation.snapshot.observed_at,
            "shadow observation identity/time regression",
        )?;
        let id = self.record_observation(&observation, &source)?;
        self.observation = observation;
        self.source = source;
        Ok(id)
    }
    pub fn source(&self) -> &ShadowSource {
        &self.source
    }
    fn record_observation(
        &self,
        observation: &ExecutionObservation,
        source: &ShadowSource,
    ) -> Result<String> {
        let value = json!({"schema_version":1,"kind":"OBSERVATION","source":source,"observation":observation});
        let hash = digest(&value)?;
        self.write_once(&format!("observation_{hash}"), &value)?;
        Ok(hash)
    }
    fn write_once(&self, id: &str, value: &serde_json::Value) -> Result<()> {
        require(valid_id(id), "invalid shadow record identity")?;
        let path = self.directory.join(format!("{id}.json"));
        let bytes =
            serde_json::to_vec(value).map_err(|_| Error("shadow record encoding failed".into()))?;
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&path) {
            Ok(mut file) => {
                file.write_all(&bytes)
                    .and_then(|()| file.sync_all())
                    .map_err(|_| Error("shadow durable write failed".into()))?;
                fs::File::open(&self.directory)
                    .and_then(|d| d.sync_all())
                    .map_err(|_| Error("shadow directory sync failed".into()))?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                let meta = fs::symlink_metadata(&path)
                    .map_err(|_| Error("shadow record unavailable".into()))?;
                require(
                    meta.is_file()
                        && !meta.file_type().is_symlink()
                        && meta.len() <= 4 * 1024 * 1024,
                    "invalid existing shadow record",
                )?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    require(
                        meta.mode() & 0o077 == 0,
                        "existing shadow recording must be private",
                    )?;
                }
                let mut existing = Vec::new();
                fs::File::open(&path)
                    .and_then(|f| f.take(4 * 1024 * 1024 + 1).read_to_end(&mut existing))
                    .map_err(|_| Error("shadow record unreadable".into()))?;
                require(
                    existing == bytes,
                    "shadow intent ID reused or recording tampered",
                )?;
            }
            Err(_) => return Err(Error("shadow durable write failed".into())),
        };
        Ok(())
    }
    /// Explicit dry-run API for the root's shadow cycle. It records the final,
    /// independently validated request without marking an execution intent sent.
    pub fn record_proposal(&self, request: &BrokerOrderRequest) -> Result<String> {
        request
            .validate()
            .map_err(|_| Error("invalid shadow proposal".into()))?;
        require(
            request.account_id == self.observation.snapshot.account_id,
            "shadow proposal account mismatch",
        )?;
        let value = json!({"schema_version":1,"kind":"PROPOSAL","source":self.source,
            "snapshot_id":self.observation.snapshot.snapshot_id,"observed_at":self.observation.snapshot.observed_at,
            "request":request,"order_mutations":0});
        let id = format!("intent_{}", request.intent_id.as_str());
        self.write_once(&id, &value)?;
        digest(&value)
    }
    pub fn observed_at(&self) -> DateTime<Utc> {
        self.observation.snapshot.observed_at
    }
}
fn validate_observation(observation: &ExecutionObservation, source: &ShadowSource) -> Result<()> {
    observation.snapshot.validate()?;
    require(
        observation.snapshot.mode == Mode::Shadow
            && observation
                .snapshot
                .account_id
                .as_str()
                .starts_with("shadow_")
            && observation.snapshot.observed_at <= Utc::now(),
        "shadow broker accepts only observed shadow-scoped snapshots",
    )?;
    match source {
        ShadowSource::Real { market_data_hash } => require(
            valid_hash(market_data_hash),
            "real shadow source hash missing",
        ),
        ShadowSource::Fixture { description } => require(
            !description.is_empty() && description.len() <= 200,
            "shadow fixture description missing",
        ),
    }
}
impl BrokerExecutionPort for ShadowBroker {
    fn submit_order(&mut self, request: &BrokerOrderRequest) -> BrokerResult<BrokerOrder> {
        self.record_proposal(request)
            .map_err(|_| BrokerFailure::new(BrokerFailureKind::InvalidRequest))?;
        Err(BrokerFailure::new(BrokerFailureKind::Unsupported))
    }
    fn cancel_order(
        &mut self,
        account: &AccountId,
        broker_order_id: &str,
    ) -> BrokerResult<CancellationReceipt> {
        if account != &self.observation.snapshot.account_id || !valid_id(broker_order_id) {
            return Err(BrokerFailure::new(BrokerFailureKind::AccountMismatch));
        }
        let value = json!({"schema_version":1,"kind":"SUPPRESSED_CANCELLATION","account_id":account,
            "broker_order_id":broker_order_id,"order_mutations":0});
        let hash =
            digest(&value).map_err(|_| BrokerFailure::new(BrokerFailureKind::InvalidRequest))?;
        self.write_once(&format!("cancel_{hash}"), &value)
            .map_err(|_| BrokerFailure::new(BrokerFailureKind::Transport))?;
        Err(BrokerFailure::new(BrokerFailureKind::Unsupported))
    }
}
impl ExecutionBroker for ShadowBroker {
    fn observe(
        &mut self,
        account: &AccountId,
        tracked: &[TrackedOrder],
    ) -> Result<ExecutionObservation> {
        require(
            account == &self.observation.snapshot.account_id,
            "shadow observation account mismatch",
        )?;
        // A missing tracked order remains uncertainty, as for any real observation.
        require(
            tracked.iter().all(|t| t.request.account_id == *account),
            "shadow tracked order scope mismatch",
        )?;
        self.record_observation(&self.observation, &self.source)?;
        Ok(self.observation.clone())
    }
}

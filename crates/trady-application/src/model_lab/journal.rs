use super::*;
use serde::de::DeserializeOwned;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub fn read_document<T: DeserializeOwned>(path: impl AsRef<Path>) -> Result<T> {
    let path = path.as_ref();
    let meta = fs::symlink_metadata(path).map_err(|_| Error("artifact unavailable".into()))?;
    require(
        meta.is_file() && !meta.file_type().is_symlink() && meta.len() <= MAX_DOCUMENT_BYTES as u64,
        "artifact must be a bounded regular file",
    )?;
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|_| Error("artifact unavailable".into()))?
        .take(MAX_DOCUMENT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Error("artifact read failed".into()))?;
    let text =
        std::str::from_utf8(&bytes).map_err(|_| Error("artifact must be UTF-8 JSON".into()))?;
    serde_json::from_value(crate::contracts::parse_json_bounded(
        text,
        MAX_DOCUMENT_BYTES,
    )?)
    .map_err(|_| Error("artifact schema invalid".into()))
}

pub(crate) fn private_directory(path: &Path) -> Result<()> {
    let existed = path.exists();
    fs::create_dir_all(path).map_err(|_| Error("artifact directory creation failed".into()))?;
    let meta =
        fs::symlink_metadata(path).map_err(|_| Error("artifact directory unavailable".into()))?;
    require(
        meta.is_dir() && !meta.file_type().is_symlink(),
        "artifact directory is not regular",
    )?;
    #[cfg(unix)]
    if !existed {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|_| Error("private artifact permissions failed".into()))?;
    }
    Ok(())
}

/// No overwrite, including concurrent writers. File and directory are synchronized.
pub fn write_immutable<T: Serialize>(path: impl AsRef<Path>, value: &T) -> Result<()> {
    let path = path.as_ref();
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    private_directory(parent)?;
    let bytes =
        serde_json::to_vec(value).map_err(|_| Error("artifact serialization failed".into()))?;
    require(
        bytes.len() <= MAX_DOCUMENT_BYTES,
        "artifact exceeds storage bound",
    )?;
    if path.exists() {
        let old: serde_json::Value = read_document(path)?;
        let new: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|_| Error("invalid encoded artifact".into()))?;
        return require(
            old == new,
            "immutable artifact identity already has different content",
        );
    }
    let temporary = parent.join(format!(".trady-model-lab-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temporary)
            .map_err(|_| Error("artifact create failed".into()))?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| Error("artifact write failed".into()))?;
        fs::hard_link(&temporary, path)
            .map_err(|_| Error("immutable artifact publication failed".into()))?;
        File::open(parent)
            .and_then(|f| f.sync_all())
            .map_err(|_| Error("artifact directory sync failed".into()))?;
        Ok(())
    })();
    let _ = fs::remove_file(&temporary);
    result
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HoldoutClaim {
    holdout_id: String,
    holdout_sha256: String,
    candidate_sha256: String,
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
}

pub struct EvaluationJournal {
    root: PathBuf,
}
impl EvaluationJournal {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        private_directory(path.as_ref())?;
        Ok(Self {
            root: path.as_ref().to_path_buf(),
        })
    }
    /// Claims are durable before scoring and survive failed evaluation. Reusing a
    /// holdout for another candidate or revised data requires a new independent set.
    pub fn evaluate(
        &self,
        candidate: &ModelCandidate,
        data: &ObservationDataset,
        now: DateTime<Utc>,
    ) -> Result<EvaluationReport> {
        candidate.validate()?;
        data.validate()?;
        let start = data
            .rows
            .first()
            .ok_or_else(|| Error("empty holdout".into()))?
            .feature_window_start_at;
        let end = data
            .rows
            .iter()
            .map(|r| r.outcome_available_at)
            .max()
            .ok_or_else(|| Error("empty holdout".into()))?;
        let claim = HoldoutClaim {
            holdout_id: data.dataset_id.clone(),
            holdout_sha256: digest(data)?,
            candidate_sha256: candidate.hash()?,
            window_start: start,
            window_end: end,
        };
        let key = digest(&(start, end))?;
        let claim_path = self.root.join(format!("{key}.claim.json"));
        let lock_path = self.root.join("evaluation.lock");
        let lock=OpenOptions::new().write(true).create_new(true).open(&lock_path)
            .map_err(|_|Error("evaluation journal busy or interrupted; inspect durable claims before operator recovery".into()))?;
        struct Guard {
            path: PathBuf,
            _file: File,
        }
        impl Drop for Guard {
            fn drop(&mut self) {
                let _ = fs::remove_file(&self.path);
            }
        }
        let guard = Guard {
            path: lock_path,
            _file: lock,
        };
        let mut seen = 0;
        for entry in
            fs::read_dir(&self.root).map_err(|_| Error("evaluation journal unavailable".into()))?
        {
            let entry = entry.map_err(|_| Error("evaluation journal unavailable".into()))?;
            if !entry.file_name().to_string_lossy().ends_with(".claim.json") {
                continue;
            }
            seen += 1;
            require(seen <= 5000, "evaluation journal claim limit exceeded")?;
            let old: HoldoutClaim = read_document(entry.path())?;
            if old.window_start <= end && start <= old.window_end {
                require(
                    old == claim,
                    "sealed holdout interval was already consumed by another candidate or revision",
                )?;
            }
        }
        if claim_path.exists() {
            let old: HoldoutClaim = read_document(&claim_path)?;
            require(
                old == claim,
                "sealed holdout was already consumed by another candidate or revision",
            )?;
        } else {
            write_immutable(&claim_path, &claim)?;
        }
        drop(guard);
        let report_path = self.root.join(format!("{key}.report.json"));
        if report_path.exists() {
            return read_document(report_path);
        }
        let report = evaluate(candidate, data, now)?;
        write_immutable(report_path, &report)?;
        Ok(report)
    }
}

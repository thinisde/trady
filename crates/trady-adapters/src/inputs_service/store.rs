use super::*;
use chrono::Duration;
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
};
use trady_application::model_lab::write_immutable;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct IndexEntry {
    id: String,
    kind: ImportKind,
    key: String,
    import_sha256: String,
    observed_at: DateTime<Utc>,
    available_at: DateTime<Utc>,
    valid_until: DateTime<Utc>,
    retain_until: DateTime<Utc>,
}

pub struct ImportStore {
    config: InputsServiceConfig,
    maximum_entries: usize,
}
impl ImportStore {
    pub fn open(config: InputsServiceConfig) -> Result<Self> {
        config.validate()?;
        for name in ["imports", "reviews", "index", "tombstones"] {
            let path = config.store_directory.join(name);
            fs::create_dir_all(&path)
                .map_err(|_| Error("input store directory unavailable".into()))?;
            ensure_directory(&path)?;
        }
        ensure_directory(&config.store_directory)?;
        Ok(Self {
            config,
            maximum_entries: MAX_IMPORTS,
        })
    }
    pub fn config(&self) -> &InputsServiceConfig {
        &self.config
    }
    #[cfg(test)]
    pub(super) fn with_capacity(mut self, maximum_entries: usize) -> Self {
        assert!((1..=MAX_IMPORTS).contains(&maximum_entries));
        self.maximum_entries = maximum_entries;
        self
    }
    // OS-owned locks are released on close/crash. Keep the lock file itself so
    // concurrent processes always lock the same inode. Readers never take this lock.
    fn lock_writes(&self) -> Result<File> {
        let path = self.config.store_directory.join(".writes.lock");
        if let Ok(metadata) = fs::symlink_metadata(&path) {
            require(
                metadata.is_file() && !metadata.file_type().is_symlink(),
                "invalid input store lock file",
            )?;
        }
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options
            .open(path)
            .map_err(|_| Error("input store write lock unavailable".into()))?;
        file.try_lock()
            .map_err(|_| Error("input store writer is busy".into()))?;
        Ok(file)
    }
    fn path(&self, kind: &str, id: &str) -> Result<PathBuf> {
        require(valid_id(id), "invalid import reference")?;
        Ok(self
            .config
            .store_directory
            .join(kind)
            .join(format!("{}.json", digest(&id)?)))
    }
    /// Stages actual provider/operator documents. Staging never makes data usable.
    pub fn import(&self, import: &SourceImport, now: DateTime<Utc>) -> Result<String> {
        let _lock = self.lock_writes()?;
        require(
            !self.path("tombstones", &import.id)?.exists(),
            "expired immutable import identity cannot be reused",
        )?;
        self.validate_import(import, now)?;
        let file = self.path("imports", &import.id)?;
        if !file.exists() {
            require(
                bounded_entries(
                    &self.config.store_directory.join("imports"),
                    self.maximum_entries,
                )?
                .len()
                    < self.maximum_entries,
                "input store import capacity reached; apply retention cleanup",
            )?;
        }
        write_immutable(file, import)?;
        digest(import)
    }
    pub fn staged(&self, id: &str) -> Result<SourceImport> {
        read_document(self.path("imports", id)?)
    }
    /// This local operator operation requires an independently authored review.
    /// Neither syntactic JSON validity nor an LLM's claimed verification is proof.
    pub fn review(&self, review: &SourceReview, now: DateTime<Utc>) -> Result<()> {
        let _lock = self.lock_writes()?;
        let import = self.staged(&review.import_id)?;
        self.validate_import(&import, now)?;
        self.validate_review(&import, review, now)?;
        // Resolve dependencies now, as well as again when the data are served.
        for id in &import.dependencies {
            let dependency = self.resolve(id, now)?;
            require(
                import.retain_until <= dependency.retain_until,
                "derived import retention exceeds an original source's rights",
            )?;
        }
        // Capacity is measured after removing expired index entries, rather than
        // against all reviews ever published. Check before either publication.
        self.prune_index(now)?;
        let index_path = self.path("index", &import.id)?;
        let active = self.index_entries(now)?;
        require(
            index_path.exists() || active.len() < self.maximum_entries,
            "active input index capacity reached",
        )?;
        write_immutable(self.path("reviews", &import.id)?, review)?;
        let entry = IndexEntry {
            id: import.id.clone(),
            kind: import.payload.kind(),
            key: import.payload.key(),
            import_sha256: digest(&import)?,
            observed_at: import.observed_at,
            available_at: import.available_at,
            valid_until: import.valid_until.min(review.valid_until),
            retain_until: import.retain_until,
        };
        write_immutable(index_path, &entry)
    }
    fn validate_import(&self, i: &SourceImport, now: DateTime<Utc>) -> Result<()> {
        let s = self
            .config
            .sources
            .get(&i.source_id)
            .ok_or_else(|| Error("unapproved source identity".into()))?;
        let url = reqwest::Url::parse(&i.canonical_url)
            .map_err(|_| Error("invalid original source URL".into()))?;
        let mut deps = BTreeSet::new();
        require(
            i.schema_version == 1
                && valid_id(&i.id)
                && valid_id(&i.origin_lineage_id)
                && i.provider == s.provider
                && s.kinds.contains(&i.payload.kind())
                && i.canonical_url.len() <= 2048
                && url.scheme() == "https"
                && url.username().is_empty()
                && url.password().is_none()
                && url.fragment().is_none()
                && url
                    .host_str()
                    .is_some_and(|h| s.allowed_hosts.iter().any(|a| a.eq_ignore_ascii_case(h)))
                && url.port_or_known_default() == Some(443)
                && i.observed_at <= i.available_at
                && i.available_at <= i.imported_at
                && i.imported_at <= now
                && i.imported_at < i.valid_until
                && now < i.valid_until
                && i.valid_until <= i.retain_until
                && now < s.rights_valid_until
                && i.retain_until <= s.rights_valid_until
                && i.imported_at
                    .checked_add_signed(Duration::hours(i64::from(s.rights.retention_hours)))
                    .is_some_and(|end| i.retain_until <= end)
                && !i.raw_document.trim().is_empty()
                && i.raw_document.len() <= MAX_IMPORT_BYTES / 2
                && trady_application::screening::hex_digest(i.raw_document.as_bytes())
                    == i.raw_document_sha256
                && i.dependencies.len() <= 128
                && i.dependencies
                    .iter()
                    .all(|id| valid_id(id) && id != &i.id && deps.insert(id)),
            "unpermitted, expired, unbound or invalid source import",
        )?;
        validate::payload(&self.config, i, now)
    }
    fn validate_review(
        &self,
        i: &SourceImport,
        r: &SourceReview,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let reviewer = self
            .config
            .reviewers
            .get(&r.reviewer_id)
            .ok_or_else(|| Error("unapproved source reviewer".into()))?;
        let source = &self.config.sources[&i.source_id];
        require(
            r.schema_version == 1
                && r.import_id == i.id
                && r.import_sha256 == digest(i)?
                && r.raw_document_sha256 == i.raw_document_sha256
                && r.reviewed_at >= i.imported_at
                && r.reviewed_at <= now
                && now < r.valid_until
                && r.valid_until <= i.valid_until
                && r.source_identity_checked
                && r.rights_checked
                && r.original_availability_checked
                && r.mapping_checked
                && r.original_lineage_checked
                && !r.model_self_attestation
                && !r.review_note.trim().is_empty()
                && r.review_note.len() <= 4000
                && reviewer.independence_group != source.independence_group,
            "independent exact-hash source review required",
        )?;
        if let ImportPayload::Evidence(e) = &i.payload {
            require(
                !e.record.primary_verified || source.may_be_primary && reviewer.may_verify_primary,
                "source/reviewer cannot attest primary evidence",
            )?;
        }
        Ok(())
    }
    pub fn resolve(&self, id: &str, now: DateTime<Utc>) -> Result<SourceImport> {
        self.resolve_inner(id, now, &mut BTreeSet::new(), &mut 0)
    }
    fn resolve_inner(
        &self,
        id: &str,
        now: DateTime<Utc>,
        visiting: &mut BTreeSet<String>,
        count: &mut usize,
    ) -> Result<SourceImport> {
        *count += 1;
        require(
            *count <= 256 && visiting.len() < 16 && visiting.insert(id.to_owned()),
            "cyclic or oversized source evidence graph",
        )?;
        let import = self.staged(id)?;
        require(import.id == id, "source import identity changed")?;
        self.validate_import(&import, now)?;
        let review: SourceReview = read_document(self.path("reviews", id)?)?;
        self.validate_review(&import, &review, now)?;
        for dependency in &import.dependencies {
            self.resolve_inner(dependency, now, visiting, count)?;
        }
        visiting.remove(id);
        Ok(import)
    }
    pub fn latest(&self, kind: ImportKind, key: &str, now: DateTime<Utc>) -> Result<SourceImport> {
        require(
            !key.is_empty() && key.len() <= 512,
            "invalid input lookup scope",
        )?;
        let mut candidates = Vec::new();
        for e in self.index_entries(now)? {
            if e.kind == kind && e.key == key && e.available_at <= now {
                candidates.push(e);
            }
        }
        candidates.sort_by(|a, b| {
            b.observed_at
                .cmp(&a.observed_at)
                .then(b.available_at.cmp(&a.available_at))
        });
        let head = candidates
            .first()
            .ok_or_else(|| Error("verified current input unavailable".into()))?;
        let import = self.resolve(&head.id, now)?;
        require(
            digest(&import)? == head.import_sha256
                && import.payload.kind() == kind
                && import.payload.key() == key,
            "verified input index content changed",
        )?;
        // Equal-time conflicting observations are unresolved; never arbitrarily select one.
        for other in candidates.iter().skip(1).take_while(|e| {
            e.observed_at == head.observed_at && e.available_at == head.available_at
        }) {
            let compared = self.resolve(&other.id, now)?;
            require(
                digest(&compared.payload)? == digest(&import.payload)?,
                "conflicting verified same-time observations",
            )?;
        }
        Ok(import)
    }
    /// Delete licensed content after its exact retention deadline; preserve only
    /// non-content hashes/identity tombstones. Expired index entries are removed;
    /// reviews and permanent identity tombstones remain outside lookup capacity.
    pub fn prune_expired(&self, now: DateTime<Utc>) -> Result<usize> {
        let _lock = self.lock_writes()?;
        let mut removed = 0;
        let imports_directory = self.config.store_directory.join("imports");
        visit_entries(&imports_directory, |path| {
            let import: SourceImport = read_document(&path)?;
            if import.retain_until <= now {
                let tombstone = serde_json::json!({"schema_version":1,"id":import.id,"import_sha256":digest(&import)?,
                    "raw_document_sha256":import.raw_document_sha256,"retained_until":import.retain_until});
                write_immutable(self.path("tombstones", &import.id)?, &tombstone)?;
                fs::remove_file(path)
                    .map_err(|_| Error("expired source content removal failed".into()))?;
                removed += 1;
            }
            Ok(())
        })?;
        if removed > 0 {
            sync_directory(&imports_directory)?;
        }
        self.prune_index(now)?;
        Ok(removed)
    }
    fn index_entry(&self, path: &Path) -> Result<IndexEntry> {
        let entry: IndexEntry = read_document(path)?;
        require(
            self.path("index", &entry.id)? == path
                && valid_hash(&entry.import_sha256)
                && !entry.key.is_empty()
                && entry.key.len() <= 512
                && entry.observed_at <= entry.available_at
                && entry.available_at < entry.valid_until
                && entry.valid_until <= entry.retain_until,
            "invalid input index metadata",
        )?;
        Ok(entry)
    }
    fn index_entries(&self, now: DateTime<Utc>) -> Result<Vec<IndexEntry>> {
        let mut active = Vec::new();
        let mut scanned = 0;
        visit_entries(&self.config.store_directory.join("index"), |path| {
            scanned += 1;
            // Lookup remains bounded even for a damaged/legacy directory. The
            // explicit streaming prune can recover arbitrarily old expired files.
            require(
                scanned <= self.maximum_entries.saturating_mul(2),
                "input index scan capacity exceeded; apply retention cleanup",
            )?;
            let entry = self.index_entry(&path)?;
            if now < entry.valid_until && now < entry.retain_until {
                require(
                    active.len() < self.maximum_entries,
                    "active input index capacity exceeded",
                )?;
                active.push(entry);
            }
            Ok(())
        })?;
        Ok(active)
    }
    // Called only while holding the write lock. Streaming, so cleanup itself is
    // not blocked by the old cumulative directory size or an active lookup cap.
    fn prune_index(&self, now: DateTime<Utc>) -> Result<()> {
        let directory = self.config.store_directory.join("index");
        let mut removed = false;
        visit_entries(&directory, |path| {
            let entry = self.index_entry(&path)?;
            if entry.valid_until <= now || entry.retain_until <= now {
                if !self.path("imports", &entry.id)?.exists()
                    && !self.path("tombstones", &entry.id)?.exists()
                {
                    // Recover an expired legacy index whose source was already
                    // removed. Retain its actual known hash; no source bytes/hash
                    // are reconstructed or invented.
                    write_immutable(
                        self.path("tombstones", &entry.id)?,
                        &serde_json::json!({
                        "schema_version":1,"id":entry.id,"import_sha256":entry.import_sha256,
                        "retained_until":entry.retain_until,"reason":"expired_index_without_source"}),
                    )?;
                }
                fs::remove_file(path)
                    .map_err(|_| Error("expired input index removal failed".into()))?;
                removed = true;
            }
            Ok(())
        })?;
        if removed {
            sync_directory(&directory)?;
        }
        Ok(())
    }
}

fn ensure_directory(path: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(path)
        .map_err(|_| Error("input store directory unavailable".into()))?;
    require(
        meta.is_dir() && !meta.file_type().is_symlink(),
        "input store directory must not be a symlink",
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // Operator-owned private store; no credentials or content in public directories.
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|_| Error("input store permissions failed".into()))?;
    }
    Ok(())
}
fn sync_directory(path: &Path) -> Result<()> {
    File::open(path)
        .and_then(|f| f.sync_all())
        .map_err(|_| Error("input store directory sync failed".into()))
}
fn bounded_entries(path: &Path, maximum: usize) -> Result<Vec<PathBuf>> {
    let mut entries = Vec::new();
    visit_entries(path, |path| {
        require(entries.len() < maximum, "input store capacity exceeded")?;
        entries.push(path);
        Ok(())
    })?;
    Ok(entries)
}
fn visit_entries(path: &Path, mut visitor: impl FnMut(PathBuf) -> Result<()>) -> Result<()> {
    for entry in fs::read_dir(path).map_err(|_| Error("input store index unavailable".into()))? {
        let entry = entry.map_err(|_| Error("input store index unreadable".into()))?;
        // Interrupted publication leaves only dot-prefixed private temp files.
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let meta = entry
            .file_type()
            .map_err(|_| Error("input store entry unavailable".into()))?;
        require(
            meta.is_file() && !meta.is_symlink(),
            "unexpected input store entry",
        )?;
        visitor(entry.path())?;
    }
    Ok(())
}

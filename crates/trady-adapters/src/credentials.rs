//! Shared credential resolution. Secret values are never serializable or printable.

use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fmt,
    fs::{File, OpenOptions},
    io::{ErrorKind, Read, Write},
    path::{Path, PathBuf},
};
use trady_application::contracts::parse_json;
use trady_domain::{Error, Result, require};

pub const DEFAULT_CREDENTIALS_FILE: &str = "config/credentials.local.json";
pub const CREDENTIALS_FILE_ENV: &str = "TRADY_CREDENTIALS_FILE";
pub const CREDENTIALS_TEMPLATE: &str = include_str!("../../../config/credentials.example.json");
pub const KNOWN_CREDENTIALS: &[&str] = &[
    "OPENCODE_API_KEY",
    "TRADY_TELEGRAM_BOT_TOKEN",
    "TRADY_T212_DEMO_READ_API_KEY",
    "TRADY_T212_DEMO_READ_API_SECRET",
    "TRADY_T212_LIVE_READ_API_KEY",
    "TRADY_T212_LIVE_READ_API_SECRET",
    "TRADY_X_BEARER_TOKEN",
    "TRADY_ALPHA_VANTAGE_API_KEY",
    "TRADY_ANALYST_DSN",
    "TRADY_EXECUTOR_DSN",
    "TRADY_GATEWAY_DSN",
    "TRADY_MIGRATOR_DSN",
    "TRADY_HASHING_SECRET_KEY",
    "TRADY_SIGNING_SECRET_KEY",
];
const MAX_FILE_BYTES: u64 = 262_144;
const MAX_SECRET_BYTES: usize = 65_536;

#[derive(Clone, Deserialize)]
#[serde(transparent)]
pub struct SecretValue(String);

impl SecretValue {
    /// Explicitly expose a secret only at the boundary that needs it.
    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}

impl fmt::Debug for SecretValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretValue([REDACTED])")
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileConfig {
    config_version: u32,
    credentials: BTreeMap<String, SecretValue>,
}

#[derive(Default)]
pub struct CredentialStore {
    credentials: BTreeMap<String, SecretValue>,
}

impl fmt::Debug for CredentialStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CredentialStore([REDACTED])")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialSource {
    Environment,
    File,
    Missing,
}

#[derive(Debug, Serialize)]
pub struct CredentialStatus {
    pub name: String,
    pub configured: bool,
    pub source: CredentialSource,
}

pub fn file_path() -> Result<PathBuf> {
    match env::var(CREDENTIALS_FILE_ENV) {
        Ok(path) => {
            require(!path.trim().is_empty(), "TRADY_CREDENTIALS_FILE is empty")?;
            Ok(path.into())
        }
        Err(env::VarError::NotPresent) => Ok(DEFAULT_CREDENTIALS_FILE.into()),
        Err(_) => Err(Error("invalid TRADY_CREDENTIALS_FILE path".into())),
    }
}

/// Create a blank private store, refusing to overwrite any existing file.
pub fn initialize() -> Result<PathBuf> {
    let path = file_path()?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&path).map_err(|error| {
        Error(
            if error.kind() == ErrorKind::AlreadyExists {
                "credential file already exists; initialization never overwrites it"
            } else {
                "cannot create credential file; its parent directory must exist"
            }
            .into(),
        )
    })?;
    file.write_all(CREDENTIALS_TEMPLATE.as_bytes())
        .map_err(|_| Error("cannot write credential file".into()))?;
    Ok(path)
}

impl CredentialStore {
    /// An absent default file preserves environment-only deployments. Explicit paths are required.
    pub fn load() -> Result<Self> {
        let path = file_path()?;
        Self::read_file(&path, env::var_os(CREDENTIALS_FILE_ENV).is_none())
    }

    pub fn from_path(path: &Path) -> Result<Self> {
        Self::read_file(path, false)
    }

    fn read_file(path: &Path, allow_missing: bool) -> Result<Self> {
        let metadata = match std::fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if allow_missing && error.kind() == ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(_) => return Err(Error("cannot read credential file".into())),
        };
        require(metadata.is_file(), "credential path must be a regular file")?;
        let file = File::open(path).map_err(|_| Error("cannot read credential file".into()))?;
        let metadata = file
            .metadata()
            .map_err(|_| Error("cannot inspect credential file".into()))?;
        require(metadata.is_file(), "credential path must be a regular file")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            require(
                metadata.permissions().mode() & 0o077 == 0,
                "credential file must have owner-only permissions; use chmod 600",
            )?;
        }
        require(
            metadata.len() <= MAX_FILE_BYTES,
            "credential file exceeds size limit",
        )?;
        let mut input = String::new();
        file.take(MAX_FILE_BYTES + 1)
            .read_to_string(&mut input)
            .map_err(|_| Error("cannot read credential file as UTF-8".into()))?;
        require(
            input.len() as u64 <= MAX_FILE_BYTES,
            "credential file exceeds size limit",
        )?;
        // The shared parser rejects duplicate keys and trailing documents. Never expose parser errors:
        // unknown fields and invalid tokens can themselves contain credential material.
        let value = parse_json(&input).map_err(|_| Error("invalid credential file JSON".into()))?;
        let config: FileConfig = serde_json::from_value(value)
            .map_err(|_| Error("invalid or unsupported credential file fields".into()))?;
        require(
            config.config_version == 1,
            "unsupported credential file version",
        )?;
        for (name, value) in &config.credentials {
            validate_name(name)?;
            validate_value(value.expose())?;
        }
        Ok(Self {
            credentials: config.credentials,
        })
    }

    fn resolve(&self, name: &str) -> Result<(Option<SecretValue>, CredentialSource)> {
        validate_name(name)?;
        let (value, source) = match env::var(name) {
            Ok(value) => (Some(SecretValue(value)), CredentialSource::Environment),
            Err(env::VarError::NotPresent) => {
                (self.credentials.get(name).cloned(), CredentialSource::File)
            }
            Err(_) => return Err(Error(format!("credential {name} is not valid UTF-8"))),
        };
        if let Some(value) = value {
            validate_value(value.expose())?;
            if !value.expose().trim().is_empty() {
                return Ok((Some(value), source));
            }
        }
        // An explicitly empty environment value disables the file value too.
        Ok((None, CredentialSource::Missing))
    }

    pub fn optional(&self, name: &str) -> Result<Option<SecretValue>> {
        self.resolve(name).map(|(value, _)| value)
    }

    pub fn required(&self, name: &str) -> Result<SecretValue> {
        self.optional(name)?.ok_or_else(|| {
            Error(format!(
                "required credential {name} is missing; set its environment variable or private credential file entry"
            ))
        })
    }

    /// Only metadata for known and file-defined names is serializable.
    pub fn status(&self) -> Result<Vec<CredentialStatus>> {
        let names: BTreeSet<&str> = KNOWN_CREDENTIALS
            .iter()
            .copied()
            .chain(self.credentials.keys().map(String::as_str))
            .collect();
        names
            .into_iter()
            .map(|name| {
                let (value, source) = self.resolve(name)?;
                Ok(CredentialStatus {
                    name: name.into(),
                    configured: value.is_some(),
                    source,
                })
            })
            .collect()
    }
}

fn validate_name(name: &str) -> Result<()> {
    require(
        !name.is_empty()
            && name.len() <= 96
            && name
                .bytes()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == b'_'),
        "invalid credential name; use 1..96 uppercase letters, digits or underscores",
    )
}

fn validate_value(value: &str) -> Result<()> {
    require(
        value.len() <= MAX_SECRET_BYTES && !value.contains('\0'),
        "credential value exceeds size limit or contains a NUL character",
    )
}

pub fn required(name: &str) -> Result<SecretValue> {
    CredentialStore::load()?.required(name)
}

pub fn optional(name: &str) -> Result<Option<SecretValue>> {
    CredentialStore::load()?.optional(name)
}

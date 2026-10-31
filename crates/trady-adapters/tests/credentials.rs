use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
use trady_adapters::credentials::{CredentialSource, CredentialStore};

struct Fixture(PathBuf);

impl Fixture {
    fn new(input: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "trady-credentials-{}-{}.json",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options
            .open(&path)
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        Self(path)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_file(&self.0).unwrap();
    }
}

#[test]
fn custom_and_multiline_secrets_are_available_only_by_explicit_access() {
    let fixture = Fixture::new(
        r#"{"config_version":1,"credentials":{
            "TRADY_TEST_PRIVATE_KEY":"private-key-first-line\nprivate-key-second-line",
            "TRADY_TEST_HASHING_KEY":"fixture-hashing-secret",
            "TRADY_TEST_EMPTY_KEY":"  "
        }}"#,
    );
    let store = CredentialStore::from_path(&fixture.0).unwrap();
    let private_key = store.required("TRADY_TEST_PRIVATE_KEY").unwrap();
    assert_eq!(
        private_key.expose(),
        "private-key-first-line\nprivate-key-second-line"
    );
    assert_eq!(
        store
            .required("TRADY_TEST_HASHING_KEY")
            .unwrap()
            .into_inner(),
        "fixture-hashing-secret"
    );
    assert!(store.optional("TRADY_TEST_EMPTY_KEY").unwrap().is_none());
    assert!(store.optional("TRADY_TEST_ABSENT_KEY").unwrap().is_none());
    assert!(store.required("TRADY_TEST_EMPTY_KEY").is_err());
    assert_eq!(format!("{private_key:?}"), "SecretValue([REDACTED])");
    assert_eq!(format!("{store:?}"), "CredentialStore([REDACTED])");
    let statuses = store.status().unwrap();
    let private_status = statuses
        .iter()
        .find(|s| s.name == "TRADY_TEST_PRIVATE_KEY")
        .unwrap();
    assert!(private_status.configured);
    assert_eq!(private_status.source, CredentialSource::File);
    let output = serde_json::to_string(&statuses).unwrap();
    assert!(!output.contains("private-key-first-line"));
    assert!(!output.contains("fixture-hashing-secret"));
}

#[test]
fn invalid_files_are_rejected_without_echoing_secret_material() {
    for input in [
        r#"{"config_version":1,"credentials":{"TRADY_TEST_KEY":"LEAK_MARKER","TRADY_TEST_KEY":"duplicate"}}"#.to_string(),
        r#"{"config_version":1,"config_version":1,"credentials":{"TRADY_TEST_KEY":"LEAK_MARKER"}}"#.into(),
        r#"{"config_version":1,"credentials":{},"LEAK_MARKER":"value"}"#.into(),
        r#"{"config_version":2,"credentials":{"TRADY_TEST_KEY":"LEAK_MARKER"}}"#.into(),
        r#"{"config_version":1,"credentials":{"TRADY_TEST_KEY":{"LEAK_MARKER":true}}}"#.into(),
        r#"{"config_version":1,"credentials":{"invalid-LEAK_MARKER":"value"}}"#.into(),
        r#"{"config_version":1,"credentials":{"TRADY_TEST_KEY":"LEAK_MARKER\u0000"}}"#.into(),
        r#"{"config_version":1,"credentials":{"TRADY_TEST_KEY":"LEAK_MARKER"}} trailing"#.into(),
        serde_json::json!({
            "config_version":1,
            "credentials":{"TRADY_TEST_KEY":"LEAK_MARKER".repeat(7000)}
        }).to_string(),
    ] {
        let fixture = Fixture::new(&input);
        let error = CredentialStore::from_path(&fixture.0).unwrap_err();
        assert!(!error.to_string().contains("LEAK_MARKER"));
    }
}

#[test]
fn oversized_files_and_invalid_lookup_names_are_rejected() {
    let fixture = Fixture::new(&" ".repeat(262_145));
    assert!(
        CredentialStore::from_path(&fixture.0)
            .unwrap_err()
            .to_string()
            .contains("size limit")
    );
    let store = CredentialStore::default();
    for name in ["", "lowercase", "KEY\nLEAK_MARKER", &"A".repeat(97)] {
        assert!(store.optional(name).is_err());
    }
}

#[cfg(unix)]
#[test]
fn group_or_other_file_access_is_rejected() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new(r#"{"config_version":1,"credentials":{}}"#);
    for mode in [0o640, 0o604, 0o666] {
        fs::set_permissions(&fixture.0, fs::Permissions::from_mode(mode)).unwrap();
        assert!(
            CredentialStore::from_path(&fixture.0)
                .unwrap_err()
                .to_string()
                .contains("owner-only permissions")
        );
    }
    fs::set_permissions(&fixture.0, fs::Permissions::from_mode(0o600)).unwrap();
    assert!(CredentialStore::from_path(&fixture.0).is_ok());
}

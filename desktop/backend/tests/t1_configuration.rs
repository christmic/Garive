#![cfg(unix)]

use std::{fs, os::unix::fs::PermissionsExt, path::Path};

use garive_desktop::{
    DesktopConfigurationError, DesktopSecretResolver, DesktopT1ConfigurationProvider,
    DESKTOP_T1_CONFIG_FILE,
};
use garive_provider_profile::SecretValue;
use tempfile::TempDir;

const SECRET: &str = "must-never-appear-in-diagnostics";

#[derive(Clone, Copy)]
struct FixtureSecrets;

impl DesktopSecretResolver for FixtureSecrets {
    fn resolve(&self, reference: &str) -> Result<SecretValue, DesktopConfigurationError> {
        if reference != "credential/t1" {
            return Err(DesktopConfigurationError::SecretUnavailable);
        }
        SecretValue::new(SECRET.to_owned())
            .map_err(|_| DesktopConfigurationError::SecretUnavailable)
    }
}

fn document() -> String {
    format!(
        r#"{{
  "schema_version": 1,
  "policy_revision": "t1-policy-v1",
  "executor_revision": "t1-executor-v1",
  "patch_recovery": "patch-recovery",
  "process_recovery": "process-recovery",
  "podman": {{
    "executable": "/usr/local/bin/podman",
    "socket_uri": "unix:///tmp/podman.sock",
    "image": "localhost/garive-tool@sha256:{}",
    "control_timeout_ms": 3000
  }},
  "process_lanes": [{{
    "name": "source-control",
    "executables": [{{"alias": "git", "path": "/usr/bin/git"}}],
    "environment": {{
      "GARIVE_LITERAL": {{"literal": "fixed"}},
      "GARIVE_TOKEN": {{"credential_ref": "credential/t1"}}
    }}
  }}]
}}"#,
        "a".repeat(64)
    )
}

fn write_document(root: &Path, value: &str) {
    fs::write(root.join(DESKTOP_T1_CONFIG_FILE), value).expect("write fixture");
}

fn provider(root: &Path) -> DesktopT1ConfigurationProvider<FixtureSecrets> {
    DesktopT1ConfigurationProvider::new(
        root.join(DESKTOP_T1_CONFIG_FILE),
        root.to_path_buf(),
        FixtureSecrets,
    )
}

#[test]
fn absent_document_does_not_enable_t1() {
    let root = TempDir::new().expect("temp root");
    assert!(provider(root.path())
        .load()
        .expect("load absence")
        .is_none());
}

#[test]
fn explicit_document_builds_exact_t1_surface_without_exposing_secrets() {
    let root = TempDir::new().expect("temp root");
    write_document(root.path(), &document());

    let host = provider(root.path())
        .load()
        .expect("load document")
        .expect("configured host");
    assert_eq!(
        host.process_lane_names().collect::<Vec<_>>(),
        ["source-control"]
    );
    assert!(!format!("{host:?}").contains(SECRET));

    let workspace = root.path().join("workspace");
    fs::create_dir(&workspace).expect("workspace");
    let execution = host
        .bind_workspace(&workspace)
        .expect("bind explicit workspace")
        .build()
        .expect("build T1 execution");
    assert_eq!(execution.capabilities().definitions.len(), 5);
}

#[test]
fn duplicate_and_unknown_members_fail_closed() {
    let root = TempDir::new().expect("temp root");
    let duplicate = document().replace(
        "\"schema_version\": 1,",
        "\"schema_version\": 1, \"schema_version\": 1,",
    );
    write_document(root.path(), &duplicate);
    assert_eq!(
        provider(root.path()).load().err(),
        Some(DesktopConfigurationError::InvalidDocument)
    );

    let unknown = document().replace(
        "{\"literal\": \"fixed\"}",
        "{\"literal\": \"fixed\", \"fallback\": \"forbidden\"}",
    );
    write_document(root.path(), &unknown);
    assert_eq!(
        provider(root.path()).load().err(),
        Some(DesktopConfigurationError::InvalidDocument)
    );
}

#[test]
fn unsafe_paths_images_permissions_and_missing_secrets_are_rejected() {
    let cases = [
        (
            "\"patch-recovery\"",
            "\"../escape\"",
            DesktopConfigurationError::InvalidValue,
        ),
        (
            &format!("localhost/garive-tool@sha256:{}", "a".repeat(64)),
            "localhost/garive-tool:latest",
            DesktopConfigurationError::ConstructionFailure,
        ),
        (
            "\"credential/t1\"",
            "\"credential/missing\"",
            DesktopConfigurationError::SecretUnavailable,
        ),
    ];
    for (from, to, expected) in cases {
        let root = TempDir::new().expect("temp root");
        write_document(root.path(), &document().replace(from, to));
        assert_eq!(provider(root.path()).load().err(), Some(expected));
    }

    let root = TempDir::new().expect("temp root");
    let recovery = root.path().join("patch-recovery");
    fs::create_dir(&recovery).expect("recovery");
    fs::set_permissions(&recovery, fs::Permissions::from_mode(0o755)).expect("permissions");
    write_document(root.path(), &document());
    assert_eq!(
        provider(root.path()).load().err(),
        Some(DesktopConfigurationError::InvalidValue)
    );
}

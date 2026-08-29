#![cfg(unix)]

use std::{fs, path::Path, process::Command};

use garive_context_pressure::{attest_clean_revision, GitAttestationConfig};
use serde_json::json;
use tempfile::tempdir;

fn git(directory: &Path, arguments: &[&str]) -> std::process::Output {
    Command::new("/usr/bin/git")
        .current_dir(directory)
        .args(arguments)
        .output()
        .unwrap()
}

fn config(directory: &Path, max_stdout_bytes: usize) -> GitAttestationConfig {
    serde_json::from_value(json!({
        "executable":"/usr/bin/git",
        "repository_path":directory,
        "timeout_ms":1000,
        "max_stdout_bytes":max_stdout_bytes,
        "max_stderr_bytes":1024
    }))
    .unwrap()
}

#[test]
fn exact_clean_head_is_the_only_accepted_provenance() {
    let directory = tempdir().unwrap();
    assert!(git(directory.path(), &["init", "--quiet"]).status.success());
    fs::write(directory.path().join("tracked.txt"), "tracked").unwrap();
    assert!(git(directory.path(), &["add", "tracked.txt"])
        .status
        .success());
    assert!(git(
        directory.path(),
        &[
            "-c",
            "user.name=Garive Test",
            "-c",
            "user.email=test@garive.invalid",
            "commit",
            "--quiet",
            "-m",
            "fixture",
        ],
    )
    .status
    .success());
    let head = String::from_utf8(git(directory.path(), &["rev-parse", "HEAD"]).stdout)
        .unwrap()
        .trim()
        .to_owned();
    assert!(attest_clean_revision(&config(directory.path(), 128), &head).is_ok());
    assert!(attest_clean_revision(&config(directory.path(), 128), &"a".repeat(40)).is_err());

    fs::write(directory.path().join("untracked.txt"), "dirty").unwrap();
    assert!(attest_clean_revision(&config(directory.path(), 128), &head).is_err());
    fs::remove_file(directory.path().join("untracked.txt")).unwrap();
    assert!(attest_clean_revision(&config(directory.path(), 1), &head).is_err());
}

#![cfg(unix)]

use std::{fs, process::Command};

use serde_json::{json, Value};
use tempfile::tempdir;

#[test]
fn explicit_cli_writes_non_publishable_evidence_without_overwrite() {
    let directory = tempdir().unwrap();
    let evidence = directory.path().join("evidence.json");
    let config_path = directory.path().join("config.json");
    let corpus = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/fixtures/agent/context-pressure-corpus-v1.json");
    let config = json!({
        "corpus_path":corpus,"evidence_path":evidence,
        "garive_revision":"test-revision","runner_revision":"context-pressure-v1","dirty":true,
        "counter":{
            "counter_id":"fixture-counter","counter_revision":"v1","publishable":false,
            "executable":"/bin/sh","argv":["-c","cat >/dev/null; printf '{\"schema_version\":1,\"input_tokens\":64}'"],
            "cwd":directory.path(),"environment":{},"timeout_ms":1000,
            "max_stdout_bytes":128,"max_stderr_bytes":128
        }
    });
    fs::write(&config_path, serde_json::to_vec(&config).unwrap()).unwrap();
    let first = run(&config_path);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let summary: Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(summary["case_count"], 4);
    assert_eq!(summary["publishable"], false);
    let document: Value = serde_json::from_slice(&fs::read(&evidence).unwrap()).unwrap();
    assert_eq!(document["contract"], "garive.context-pressure-evidence");
    assert_eq!(document["publishable"], false);
    assert_eq!(document["cases"].as_array().unwrap().len(), 4);
    assert_eq!(document["classes"].as_array().unwrap().len(), 4);
    assert!(document.get("environment").is_none());
    let before = fs::read(&evidence).unwrap();
    let second = run(&config_path);
    assert!(!second.status.success());
    assert_eq!(fs::read(&evidence).unwrap(), before);
}

#[test]
fn dirty_publication_and_unknown_configuration_fail_before_output() {
    let directory = tempdir().unwrap();
    let config_path = directory.path().join("invalid.json");
    fs::write(
        &config_path,
        json!({"corpus_path":"missing","evidence_path":directory.path().join("out"),
            "garive_revision":"revision","runner_revision":"runner","dirty":true,
            "counter":{"counter_id":"counter","counter_revision":"v1","publishable":true,
                "executable":"/bin/false","argv":[],"cwd":directory.path(),"environment":{},
                "timeout_ms":1,"max_stdout_bytes":1,"max_stderr_bytes":1}})
        .to_string(),
    )
    .unwrap();
    let output = run(&config_path);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid_provenance"));
}

fn run(config: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_garive-context-pressure"))
        .args(["run", config.to_str().unwrap()])
        .output()
        .unwrap()
}

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
            "kind":"command",
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
fn existing_evidence_fails_before_counter_process_side_effect() {
    let directory = tempdir().unwrap();
    let evidence = directory.path().join("evidence.json");
    let marker = directory.path().join("counter-started");
    let config_path = directory.path().join("config.json");
    let corpus = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/fixtures/agent/context-pressure-corpus-v1.json");
    fs::write(&evidence, b"original").unwrap();
    fs::write(
        &config_path,
        json!({
            "corpus_path":corpus,"evidence_path":evidence,
            "garive_revision":"test-revision","runner_revision":"context-pressure-v1",
            "dirty":true,"counter":{"kind":"command","counter_id":"fixture-counter",
                "counter_revision":"v1","publishable":false,"executable":"/bin/sh",
                "argv":["-c",format!("touch {}; exit 1", marker.display())],
                "cwd":directory.path(),"environment":{},"timeout_ms":1000,
                "max_stdout_bytes":128,"max_stderr_bytes":128}
        })
        .to_string(),
    )
    .unwrap();

    let output = run(&config_path);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("evidence_create_failed"));
    assert!(!marker.exists());
    assert_eq!(fs::read(&evidence).unwrap(), b"original");
}

#[test]
fn dirty_publication_and_unknown_configuration_fail_before_output() {
    let directory = tempdir().unwrap();
    let config_path = directory.path().join("invalid.json");
    fs::write(
        &config_path,
        json!({"corpus_path":"missing","evidence_path":directory.path().join("out"),
            "garive_revision":"revision","runner_revision":"runner","dirty":true,
            "counter":{"kind":"anthropic_messages_exact",
                "counter_revision":"v1","publishable":true,"credential_ref":"never-read",
                "target_id":"target","model_id":"model","capabilities":["text","tools"],
                "projection_max_output_tokens":1,"extra_headers":[],
                "http":{"connect_timeout_ms":1,"request_timeout_ms":1,"max_response_bytes":1}}})
        .to_string(),
    )
    .unwrap();
    let output = run(&config_path);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid_provenance"));

    let mut clean_without_attestation: Value =
        serde_json::from_slice(&fs::read(&config_path).unwrap()).unwrap();
    clean_without_attestation["dirty"] = json!(false);
    fs::write(
        &config_path,
        serde_json::to_vec(&clean_without_attestation).unwrap(),
    )
    .unwrap();
    let output = run(&config_path);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid_provenance"));
}

#[test]
fn provider_template_parses_and_stops_before_secret_on_unattested_revision() {
    let template =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("config.provider-reference-v1.json");
    let output = run(&template);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid_provenance"));
}

fn run(config: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_garive-context-pressure"))
        .args(["run", config.to_str().unwrap()])
        .output()
        .unwrap()
}

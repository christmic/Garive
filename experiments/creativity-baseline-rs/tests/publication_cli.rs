use std::{fs, process::Command};

use serde_json::{json, Value};
use tempfile::tempdir;

#[test]
fn publication_template_parses_and_fails_attestation_before_secret_or_network() {
    let template = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("config.publication-reference-v1.json");
    let output = run(&template);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid_provenance"));
}

#[test]
fn dirty_loopback_and_unknown_configurations_fail_without_evidence() {
    let directory = tempdir().unwrap();
    let template = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("config.publication-reference-v1.json");
    let mut config: Value = serde_json::from_slice(&fs::read(template).unwrap()).unwrap();
    let evidence = directory.path().join("evidence.json");
    let config_path = directory.path().join("config.json");
    config["evidence_path"] = json!(evidence);
    config["dirty"] = json!(true);
    fs::write(&config_path, serde_json::to_vec(&config).unwrap()).unwrap();
    let output = run(&config_path);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid_provenance"));
    assert!(!evidence.exists());

    config["dirty"] = json!(false);
    config["generator"]["endpoint"] = json!("http://127.0.0.1:9/v1/responses");
    fs::write(&config_path, serde_json::to_vec(&config).unwrap()).unwrap();
    let output = run(&config_path);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid_provenance"));
    assert!(!evidence.exists());

    config["unknown"] = json!(true);
    fs::write(&config_path, serde_json::to_vec(&config).unwrap()).unwrap();
    let output = run(&config_path);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid_configuration"));
    assert!(!evidence.exists());
}

fn run(config: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_garive-creativity-publication"))
        .args(["run", config.to_str().unwrap()])
        .output()
        .unwrap()
}

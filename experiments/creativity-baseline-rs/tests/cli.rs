use std::{fs, process::Command};

use serde_json::{json, Value};
use tempfile::tempdir;

#[test]
fn fixture_cli_writes_deterministic_content_free_evidence_without_overwrite() {
    let directory = tempdir().unwrap();
    let evidence = directory.path().join("evidence.json");
    let config_path = directory.path().join("config.json");
    let corpus = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/fixtures/eval/creativity-corpus-v1.json");
    let command = |executable: &str, implementation_id: &str| {
        json!({
            "implementation_id":implementation_id,"implementation_revision":"v1",
            "publishable":false,"executable":executable,"argv":[],
            "cwd":directory.path(),"environment":{},"timeout_ms":1000,
            "max_stdout_bytes":65536,"max_stderr_bytes":4096
        })
    };
    let config = json!({
        "corpus_path":corpus,"evidence_path":evidence,
        "garive_revision":"test-revision","runner_revision":"cr-a-v1",
        "dirty":true,"seed":20260830,
        "generator":command(env!("CARGO_BIN_EXE_garive-fixture-creativity-generator"),
            "fixture-generator"),
        "evaluator":command(env!("CARGO_BIN_EXE_garive-fixture-creativity-evaluator"),
            "fixture-evaluator")
    });
    fs::write(&config_path, serde_json::to_vec(&config).unwrap()).unwrap();
    let first = run(&config_path);
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let result: Value = serde_json::from_slice(&first.stdout).unwrap();
    assert_eq!(result["task_count"], 4);
    assert_eq!(result["publishable"], false);

    let bytes = fs::read(&evidence).unwrap();
    let document: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(document["contract"], "garive.creativity-baseline-evidence");
    assert_eq!(document["maturity"], "non_publication_prerequisite");
    assert_eq!(document["publishable"], false);
    assert_eq!(document["pairs"].as_array().unwrap().len(), 4);
    assert_eq!(document["summary"]["control"]["candidate_count"], 4);
    assert_eq!(
        document["summary"]["bounded_alternatives"]["candidate_count"],
        12
    );
    assert_eq!(
        document["summary"]["bounded_alternatives"]["correct_cluster_mean_numerator"],
        12
    );
    assert_eq!(document["summary"]["classes"].as_array().unwrap().len(), 4);
    let evidence_text = String::from_utf8(bytes.clone()).unwrap();
    for forbidden in [
        "generator_prompt",
        "evaluator_rubric",
        "fixture alternative",
        "environment",
        "required_constraints",
        "selected_candidate_id",
    ] {
        assert!(!evidence_text.contains(forbidden), "leaked {forbidden}");
    }
    let second = run(&config_path);
    assert!(!second.status.success());
    assert_eq!(fs::read(&evidence).unwrap(), bytes);
}

#[test]
fn unknown_configuration_fails_before_evidence_creation() {
    let directory = tempdir().unwrap();
    let config = directory.path().join("invalid.json");
    let evidence = directory.path().join("evidence.json");
    fs::write(
        &config,
        json!({"corpus_path":"missing","evidence_path":evidence,
            "garive_revision":"revision","runner_revision":"runner","dirty":true,
            "seed":1,"generator":{},"evaluator":{},"unknown":true})
        .to_string(),
    )
    .unwrap();
    let output = run(&config);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid_configuration"));
    assert!(!evidence.exists());
}

fn run(config: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_garive-creativity-baseline"))
        .args(["run", config.to_str().unwrap()])
        .output()
        .unwrap()
}

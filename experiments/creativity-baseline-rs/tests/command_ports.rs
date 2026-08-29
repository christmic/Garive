#![cfg(unix)]

use std::{collections::BTreeMap, path::PathBuf};

use garive_creativity_baseline::{
    load_creativity_corpus, run_creativity_baseline, CommandCreativityEvaluator,
    CommandCreativityGenerator, CommandPortConfig, CreativityGeneratorPort, GeneratorRequest,
};
use garive_eval::{CreativityArm, EvaluationCaseId};

const CORPUS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../spec/fixtures/eval/creativity-corpus-v1.json"
));

fn config(id: &str, script: &str) -> CommandPortConfig {
    CommandPortConfig {
        implementation_id: id.into(),
        implementation_revision: "fixture-v1".into(),
        publishable: false,
        executable: PathBuf::from("/bin/sh"),
        argv: vec!["-c".into(), script.into()],
        cwd: std::env::current_dir().unwrap(),
        environment: BTreeMap::new(),
        timeout_ms: 1000,
        max_stdout_bytes: 4096,
        max_stderr_bytes: 128,
    }
}

const GENERATOR: &str = r#"
    [ -z "${HOME+x}" ] || exit 10
    payload=$(cat)
    case "$payload" in *evaluator_rubric*) exit 11;; esac
    case "$payload" in
      *'"arm":"control"'*)
        printf '{"schema_version":1,"candidates":[{"candidate_id":"c0","content":"valid control"}],"selected_candidate_id":"c0"}' ;;
      *)
        printf '{"schema_version":1,"candidates":[{"candidate_id":"c0","content":"valid first"},{"candidate_id":"c1","content":"valid second"}],"selected_candidate_id":"c1"}' ;;
    esac
"#;

const EVALUATOR: &str = r#"
    [ -z "${HOME+x}" ] || exit 20
    payload=$(cat)
    case "$payload" in *'"arm"'*|*selected_candidate_id*) exit 21;; esac
    case "$payload" in
      *'"candidate_id":"c1"'*)
        printf '{"schema_version":1,"verdicts":[{"candidate_id":"c0","correct":true,"correct_cluster_id":"cluster-a"},{"candidate_id":"c1","correct":true,"correct_cluster_id":"cluster-b"}]}' ;;
      *)
        printf '{"schema_version":1,"verdicts":[{"candidate_id":"c0","correct":true,"correct_cluster_id":"cluster-a"}]}' ;;
    esac
"#;

#[test]
fn command_ports_clear_environment_and_preserve_blind_paired_wire() {
    let generator = CommandCreativityGenerator::new(config("generator", GENERATOR)).unwrap();
    let evaluator = CommandCreativityEvaluator::new(config("evaluator", EVALUATOR)).unwrap();
    let corpus = load_creativity_corpus(CORPUS.as_bytes()).unwrap();
    let run = run_creativity_baseline(&corpus, &generator, &evaluator, 7).unwrap();
    assert!(!run.generator.publishable);
    assert!(!run.evaluator.publishable);
    assert_ne!(run.generator.config_digest, run.evaluator.config_digest);
    assert_eq!(run.summary.control.candidate_count, 4);
    assert_eq!(run.summary.bounded_alternatives.candidate_count, 8);
    assert_eq!(
        run.summary
            .bounded_alternatives
            .correct_cluster_mean_numerator,
        8
    );
    assert_eq!(
        run.summary.bounded_alternatives.selected_correct_numerator,
        4
    );
}

#[test]
fn command_construction_and_output_failures_are_closed() {
    let mut publication = config("generator", GENERATOR);
    publication.publishable = true;
    assert!(CommandCreativityGenerator::new(publication).is_err());

    for script in [
        "exit 2",
        "while :; do :; done",
        "printf '{\"schema_version\":1,\"candidates\":[],\"selected_candidate_id\":\"x\",\"extra\":true}'",
        "printf 'xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx' >&2",
    ] {
        let mut value = config("generator", script);
        value.timeout_ms = 10;
        value.max_stderr_bytes = 8;
        let generator = CommandCreativityGenerator::new(value).unwrap();
        assert!(generator
            .generate(GeneratorRequest {
                task_id: &EvaluationCaseId::new("task").unwrap(),
                arm: CreativityArm::Control,
                prompt: "prompt",
                seed: 1,
                max_candidates: 1,
                max_candidate_utf8_bytes: 128,
                max_total_candidate_utf8_bytes: 128,
            })
            .is_err());
    }
}

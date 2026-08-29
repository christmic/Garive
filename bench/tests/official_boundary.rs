use std::sync::Mutex;

use bench::{
    AgentOutput, BenchError, BenchErrorCode, ExactSweIntake, IntakeAdapter, OfficialEvaluator,
    OfficialEvaluatorConfig, OfficialInvocation, OfficialProcess, OfficialProcessOutput,
    PatchAdapter, SweBenchOfficialEvaluator, SweCase, SweDataset, UnifiedDiffPatchAdapter,
    WorkspaceLease,
};
use futures::executor::block_on;
use garive_eval::EvaluationCaseId;
use serde_json::{json, Value};

struct Process {
    output: Mutex<Option<Result<OfficialProcessOutput, BenchError>>>,
    invocation: Mutex<Option<OfficialInvocation>>,
}

impl OfficialProcess for Process {
    fn invoke<'a>(
        &'a self,
        invocation: OfficialInvocation,
    ) -> bench::BenchFuture<'a, OfficialProcessOutput> {
        Box::pin(async move {
            *self.invocation.lock().unwrap() = Some(invocation);
            self.output.lock().unwrap().take().unwrap()
        })
    }
}

#[test]
fn intake_is_gold_free_and_patch_adapter_is_strict() {
    let case = case();
    let workspace = WorkspaceLease {
        handle: "workspace".into(),
        case_id: case.instance_id.as_str().into(),
        base_commit: case.base_commit.clone(),
    };
    let input = block_on(ExactSweIntake.translate(&case, &workspace)).unwrap();
    assert_eq!(input.payload, case.problem_statement);
    assert!(!input.payload.contains("FAIL_TO_PASS"));
    let adapter = UnifiedDiffPatchAdapter::new(1_024).unwrap();
    let valid = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n";
    assert_eq!(
        block_on(adapter.translate(&output(valid), &case)).unwrap(),
        valid
    );
    for invalid in [
        "",
        "--- a/x\n+++ b/x\n",
        "diff --git a/../x b/../x\n",
        "diff --git a/.git/config b/.git/config\n",
        "diff --git a/x b/x\r\n",
        "diff --git a/x b/x\nGIT binary patch\n",
    ] {
        assert_eq!(
            block_on(adapter.translate(&output(invalid), &case))
                .unwrap_err()
                .code(),
            BenchErrorCode::InvalidPatch
        );
    }
}

#[test]
fn official_invocation_and_prediction_are_exact() {
    let process = Process {
        output: Mutex::new(Some(Ok(OfficialProcessOutput {
            exit_code: 0,
            report_json: report(true),
        }))),
        invocation: Mutex::new(None),
    };
    let evaluator = SweBenchOfficialEvaluator::new(config(), &process).unwrap();
    let verdict = block_on(evaluator.evaluate(&case(), patch())).unwrap();
    assert_eq!(verdict, bench::EvaluationVerdict::Passed);
    let invocation = process.invocation.lock().unwrap().take().unwrap();
    assert_eq!(invocation.executable, "/opt/venv/bin/python");
    assert_eq!(invocation.working_directory, "/runs/run-1");
    assert_eq!(
        invocation.environment,
        [("DOCKER_HOST".into(), "unix:///docker.sock".into())]
    );
    assert_eq!(
        invocation.arguments,
        [
            "-m",
            "swebench.harness.run_evaluation",
            "--dataset_name",
            "SWE-bench/SWE-bench_Lite",
            "--split",
            "test",
            "--predictions_path",
            "/runs/run-1/predictions.jsonl",
            "--instance_ids",
            "case-1",
            "--max_workers",
            "2",
            "--run_id",
            "run-1",
            "--timeout",
            "1800",
            "--cache_level",
            "env",
            "--clean",
            "true"
        ]
    );
    let prediction: Value = serde_json::from_slice(&invocation.prediction_jsonl).unwrap();
    assert_eq!(
        prediction,
        json!({"instance_id":"case-1","model_name_or_path":"garive-1",
        "model_patch":patch()})
    );
    assert!(invocation.prediction_jsonl.ends_with(b"\n"));
}

#[test]
fn report_exit_schema_and_coverage_fail_closed() {
    for output in [
        OfficialProcessOutput {
            exit_code: 1,
            report_json: report(true),
        },
        OfficialProcessOutput {
            exit_code: 0,
            report_json: b"{}".to_vec(),
        },
        OfficialProcessOutput {
            exit_code: 0,
            report_json: String::from_utf8(report(true))
                .unwrap()
                .replace("\"schema_version\":2", "\"schema_version\":3")
                .into_bytes(),
        },
        OfficialProcessOutput {
            exit_code: 0,
            report_json: String::from_utf8(report(true))
                .unwrap()
                .replace("\"completed_ids\":[\"case-1\"]", "\"completed_ids\":[]")
                .into_bytes(),
        },
    ] {
        let process = Process {
            output: Mutex::new(Some(Ok(output))),
            invocation: Mutex::new(None),
        };
        let evaluator = SweBenchOfficialEvaluator::new(config(), &process).unwrap();
        assert_eq!(
            block_on(evaluator.evaluate(&case(), patch()))
                .unwrap_err()
                .code(),
            BenchErrorCode::InvalidEvaluation
        );
    }
    let process = Process {
        output: Mutex::new(Some(Ok(OfficialProcessOutput {
            exit_code: 0,
            report_json: report(false),
        }))),
        invocation: Mutex::new(None),
    };
    let evaluator = SweBenchOfficialEvaluator::new(config(), &process).unwrap();
    assert_eq!(
        block_on(evaluator.evaluate(&case(), patch())).unwrap(),
        bench::EvaluationVerdict::Failed
    );
}

#[test]
fn official_configuration_has_no_implicit_defaults() {
    let process = Process {
        output: Mutex::new(None),
        invocation: Mutex::new(None),
    };
    let mut invalid = config();
    invalid.jobs = 1;
    assert_eq!(
        SweBenchOfficialEvaluator::new(invalid, &process)
            .err()
            .unwrap()
            .code(),
        BenchErrorCode::InvalidEvaluation
    );
    let mut duplicate_environment = config();
    duplicate_environment
        .environment
        .push(("DOCKER_HOST".into(), "other".into()));
    assert_eq!(
        SweBenchOfficialEvaluator::new(duplicate_environment, &process)
            .err()
            .unwrap()
            .code(),
        BenchErrorCode::InvalidEvaluation
    );
}

fn config() -> OfficialEvaluatorConfig {
    OfficialEvaluatorConfig {
        python_executable: "/opt/venv/bin/python".into(),
        dataset: SweDataset::Lite,
        predictions_path: "/runs/run-1/predictions.jsonl".into(),
        run_directory: "/runs/run-1".into(),
        run_id: "run-1".into(),
        model_name: "garive-1".into(),
        jobs: 2,
        timeout_seconds: 1_800,
        environment: vec![("DOCKER_HOST".into(), "unix:///docker.sock".into())],
    }
}

fn case() -> SweCase {
    SweCase {
        instance_id: EvaluationCaseId::new("case-1").unwrap(),
        repository: "owner/repo".into(),
        base_commit: "a".repeat(40),
        problem_statement: "Fix the public bug.".into(),
        version: "1".into(),
        fail_to_pass: vec!["test_fix".into()],
        pass_to_pass: vec!["test_regression".into()],
    }
}

fn patch() -> &'static str {
    "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1 @@\n-old\n+new\n"
}
fn output(raw: &str) -> AgentOutput {
    AgentOutput {
        raw: raw.into(),
        duration_ms: 1,
        input_tokens: None,
        output_tokens: None,
    }
}
fn report(resolved: bool) -> Vec<u8> {
    let resolved_ids = if resolved { vec!["case-1"] } else { vec![] };
    let unresolved_ids = if resolved { vec![] } else { vec!["case-1"] };
    serde_json::to_vec(&json!({"total_instances":1,"submitted_instances":1,"completed_instances":1,
        "resolved_instances":u8::from(resolved),"unresolved_instances":u8::from(!resolved),
        "empty_patch_instances":0,"error_instances":0,"completed_ids":["case-1"],"incomplete_ids":[],
        "empty_patch_ids":[],"submitted_ids":["case-1"],"resolved_ids":resolved_ids,
        "unresolved_ids":unresolved_ids,"error_ids":[],"schema_version":2})).unwrap()
}

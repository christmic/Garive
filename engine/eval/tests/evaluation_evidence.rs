use garive_eval::{
    summarize, EvaluationBaseline, EvaluationBaselineProvenance, EvaluationCaseId,
    EvaluationCaseOutcome, EvaluationCaseResult, EvaluationErrorCode, EvaluationLimits,
    EvaluationRunId, EvaluationSuiteId,
};
use serde_json::Value;

const FIXTURE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../spec/fixtures/agent/evaluation-summary-v1.json"
));

#[test]
fn shared_fixture_reduces_agent_and_infrastructure_evidence_exactly() {
    let fixture: Value = serde_json::from_str(FIXTURE).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let results: Vec<_> = case["results"]
            .as_array()
            .unwrap()
            .iter()
            .map(result)
            .collect();
        let summary = summarize(
            &results,
            EvaluationLimits {
                max_cases: 16,
                max_case_duration_ms: 1_000,
            },
        )
        .unwrap();
        let expected = &case["expected"];
        assert_eq!(summary.attempted, expected["attempted"].as_u64().unwrap());
        assert_eq!(summary.passed, expected["passed"].as_u64().unwrap());
        assert_eq!(summary.failed, expected["failed"].as_u64().unwrap());
        assert_eq!(
            summary.infrastructure_failed,
            expected["infrastructure_failed"].as_u64().unwrap()
        );
        assert_eq!(
            summary.not_attempted,
            expected["not_attempted"].as_u64().unwrap()
        );
        assert_eq!(
            summary.score.map(|score| score.numerator),
            expected["score_numerator"].as_u64()
        );
        assert_eq!(
            summary.score.map(|score| score.denominator),
            expected["score_denominator"].as_u64()
        );
    }
}

#[test]
fn reduction_is_order_independent_but_duplicate_ids_fail() {
    let mut results = vec![passed("a"), failed("b"), infrastructure("c")];
    let forward = summary(&results);
    results.reverse();
    assert_eq!(summary(&results), forward);
    results.push(passed("a"));
    assert_eq!(
        summarize(&results, limits()).unwrap_err().code(),
        EvaluationErrorCode::DuplicateCase
    );
}

#[test]
fn identities_limits_duration_and_not_attempted_evidence_fail_closed() {
    assert_eq!(
        EvaluationCaseId::new("").unwrap_err().code(),
        EvaluationErrorCode::EmptyIdentity
    );
    assert_eq!(
        EvaluationCaseId::new("x".repeat(257)).unwrap_err().code(),
        EvaluationErrorCode::IdentityTooLong
    );
    assert_eq!(
        summarize(
            &[passed("a")],
            EvaluationLimits {
                max_cases: 0,
                max_case_duration_ms: 1
            }
        )
        .unwrap_err()
        .code(),
        EvaluationErrorCode::InvalidLimits
    );
    let mut slow = passed("a");
    slow.duration_ms = 1_001;
    assert_eq!(
        summarize(&[slow], limits()).unwrap_err().code(),
        EvaluationErrorCode::DurationExceeded
    );
    let invalid_skip = EvaluationCaseResult {
        case_id: EvaluationCaseId::new("skip").unwrap(),
        outcome: EvaluationCaseOutcome::NotAttempted,
        duration_ms: 1,
        input_tokens: None,
        output_tokens: None,
    };
    assert_eq!(
        summarize(&[invalid_skip], limits()).unwrap_err().code(),
        EvaluationErrorCode::InvalidNotAttempted
    );
}

#[test]
fn baseline_requires_clean_complete_digest_bound_provenance() {
    let provenance = EvaluationBaselineProvenance {
        run_id: EvaluationRunId::new("run-1").unwrap(),
        suite_id: EvaluationSuiteId::new("swe-bench-lite").unwrap(),
        dataset_revision: "SWE-bench/SWE-bench_Lite:test".into(),
        harness_revision: "7a21e05772954cc81471ae19d56f436cecf43c54".into(),
        agent_revision: "feature-revision".into(),
        dirty: false,
        config_digest: "A".repeat(64),
    };
    let baseline = EvaluationBaseline::new(provenance.clone(), summary(&[passed("a")])).unwrap();
    assert_eq!(baseline.config_digest, "a".repeat(64));
    assert_eq!(baseline.summary.score.unwrap().numerator, 1);
    assert_eq!(
        EvaluationBaseline::new(
            EvaluationBaselineProvenance {
                dirty: true,
                ..provenance
            },
            summary(&[passed("a")])
        )
        .unwrap_err()
        .code(),
        EvaluationErrorCode::InvalidBaseline
    );
}

fn result(value: &Value) -> EvaluationCaseResult {
    EvaluationCaseResult {
        case_id: EvaluationCaseId::new(value["case_id"].as_str().unwrap()).unwrap(),
        outcome: match value["outcome"].as_str().unwrap() {
            "passed" => EvaluationCaseOutcome::Passed,
            "failed" => EvaluationCaseOutcome::Failed,
            "infrastructure_failure" => EvaluationCaseOutcome::InfrastructureFailure,
            "not_attempted" => EvaluationCaseOutcome::NotAttempted,
            other => panic!("unknown fixture outcome {other}"),
        },
        duration_ms: value["duration_ms"].as_u64().unwrap(),
        input_tokens: value["input_tokens"].as_u64(),
        output_tokens: value["output_tokens"].as_u64(),
    }
}

fn passed(id: &str) -> EvaluationCaseResult {
    terminal(id, EvaluationCaseOutcome::Passed)
}

fn failed(id: &str) -> EvaluationCaseResult {
    terminal(id, EvaluationCaseOutcome::Failed)
}

fn infrastructure(id: &str) -> EvaluationCaseResult {
    terminal(id, EvaluationCaseOutcome::InfrastructureFailure)
}

fn terminal(id: &str, outcome: EvaluationCaseOutcome) -> EvaluationCaseResult {
    EvaluationCaseResult {
        case_id: EvaluationCaseId::new(id).unwrap(),
        outcome,
        duration_ms: 10,
        input_tokens: None,
        output_tokens: None,
    }
}

fn limits() -> EvaluationLimits {
    EvaluationLimits {
        max_cases: 16,
        max_case_duration_ms: 1_000,
    }
}

fn summary(results: &[EvaluationCaseResult]) -> garive_eval::EvaluationSummary {
    summarize(results, limits()).unwrap()
}

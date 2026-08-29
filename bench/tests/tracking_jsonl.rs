use bench::{JsonlResultSink, ResultSink, TrackingCompletion, TrackingDescriptor};
use futures::executor::block_on;
use garive_eval::{
    EvaluationCaseId, EvaluationCaseOutcome, EvaluationCaseResult, EvaluationLimits,
};
use serde_json::Value;

#[test]
fn jsonl_is_exact_source_order_and_published_baseline_is_clean() {
    let sink = JsonlResultSink::new(Vec::new(), descriptor(true)).unwrap();
    assert!(block_on(sink.append(1, &failed("b"))).is_err());
    block_on(sink.append(0, &passed("a"))).unwrap();
    block_on(sink.append(1, &failed("b"))).unwrap();
    let completion = sink
        .finish(&[passed("a"), failed("b")], 25, limits())
        .unwrap();
    let TrackingCompletion::Published(baseline) = completion else {
        panic!("published baseline expected")
    };
    assert_eq!(baseline.summary.score.unwrap().numerator, 1);
    assert_eq!(baseline.summary.score.unwrap().denominator, 2);
    assert_eq!(baseline.config_digest, "a".repeat(64));
    let bytes = sink.into_writer().unwrap();
    let lines = String::from_utf8(bytes)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(lines.len(), 4);
    assert_eq!(lines[0]["kind"], "run-start");
    assert_eq!(lines[1]["source_index"], 0);
    assert_eq!(lines[2]["source_index"], 1);
    assert_eq!(lines[3]["kind"], "run-end");
    assert_eq!(lines[3]["score_numerator"], 1);
    assert_eq!(lines[3]["score_denominator"], 2);
}

#[test]
fn smoke_is_never_promoted_and_incomplete_or_duplicate_finish_fails() {
    let mut smoke = descriptor(false);
    smoke.dirty = true;
    smoke.environment_kind = "self-cow".into();
    smoke.jobs = 1;
    let sink = JsonlResultSink::new(Vec::new(), smoke).unwrap();
    block_on(sink.append(0, &passed("a"))).unwrap();
    assert!(sink.finish(&[passed("a")], 1, limits()).is_err());
    block_on(sink.append(1, &failed("b"))).unwrap();
    assert!(matches!(
        sink.finish(&[passed("a"), failed("b")], 2, limits())
            .unwrap(),
        TrackingCompletion::Development(_)
    ));
    assert!(sink
        .finish(&[passed("a"), failed("b")], 2, limits())
        .is_err());
}

#[test]
fn invalid_publication_provenance_fails_before_any_output() {
    for mutate in ["dirty", "self-cow", "sequential", "digest"] {
        let mut value = descriptor(true);
        match mutate {
            "dirty" => value.dirty = true,
            "self-cow" => value.environment_kind = "self-cow".into(),
            "sequential" => value.jobs = 1,
            "digest" => value.config_digest = "bad".into(),
            _ => unreachable!(),
        }
        assert!(JsonlResultSink::new(Vec::new(), value).is_err());
    }
}

fn descriptor(publishable: bool) -> TrackingDescriptor {
    TrackingDescriptor {
        run_id: "run-1".into(),
        suite_id: "swe-bench-lite".into(),
        dataset_revision: "SWE-bench/SWE-bench_Lite:test@revision".into(),
        harness_revision: "7a21e05772954cc81471ae19d56f436cecf43c54".into(),
        agent_revision: "agent-revision".into(),
        dirty: false,
        config_digest: "A".repeat(64),
        intake_adapter: "exact-swe-v1".into(),
        patch_adapter: "unified-diff-v1".into(),
        environment_kind: "official".into(),
        jobs: 2,
        case_count: 2,
        publishable,
    }
}

fn passed(id: &str) -> EvaluationCaseResult {
    result(id, EvaluationCaseOutcome::Passed)
}
fn failed(id: &str) -> EvaluationCaseResult {
    result(id, EvaluationCaseOutcome::Failed)
}
fn result(id: &str, outcome: EvaluationCaseOutcome) -> EvaluationCaseResult {
    EvaluationCaseResult {
        case_id: EvaluationCaseId::new(id).unwrap(),
        outcome,
        duration_ms: 10,
        input_tokens: Some(3),
        output_tokens: Some(2),
    }
}
fn limits() -> EvaluationLimits {
    EvaluationLimits {
        max_cases: 2,
        max_case_duration_ms: 100,
    }
}

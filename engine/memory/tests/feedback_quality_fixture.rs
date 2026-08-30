use std::path::PathBuf;

use garive_memory::{
    evaluate_recall_feedback_quality, RecallFeedbackOutcome, RecallFeedbackQualityRequest,
    RecallFeedbackRow, RecallQualityRatio,
};
use serde_json::Value;

#[test]
fn shared_feedback_quality_is_exact_and_fail_closed() {
    let fixture: Value = serde_json::from_str(
        &std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../spec/fixtures/agent/memory-recall-feedback-v1.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let request = request(&fixture);
    let actual = evaluate_recall_feedback_quality(&request).unwrap();
    let expected = &fixture["expected"];
    assert_eq!(actual.exposures, expected["exposures"].as_u64().unwrap());
    assert_eq!(
        actual.applications,
        expected["applications"].as_u64().unwrap()
    );
    assert_eq!(actual.censored, expected["censored"].as_u64().unwrap());
    assert_eq!(actual.pending, expected["pending"].as_u64().unwrap());
    assert_eq!(actual.verified, expected["verified"].as_u64().unwrap());
    assert_eq!(actual.falsified, expected["falsified"].as_u64().unwrap());
    assert_eq!(actual.neutral, expected["neutral"].as_u64().unwrap());
    assert_eq!(
        actual.application_ratio,
        Some(ratio(expected, "application_ratio"))
    );
    assert_eq!(
        actual.verified_outcome_ratio,
        Some(ratio(expected, "verified_outcome_ratio"))
    );

    let mut outcome_without_application = request.clone();
    outcome_without_application.rows[0].outcome = Some(RecallFeedbackOutcome::Verified);
    assert!(evaluate_recall_feedback_quality(&outcome_without_application).is_err());
    let mut unordered = request.clone();
    unordered.rows.swap(0, 1);
    assert!(evaluate_recall_feedback_quality(&unordered).is_err());
    let empty = RecallFeedbackQualityRequest {
        rows: vec![],
        ..request
    };
    let empty = evaluate_recall_feedback_quality(&empty).unwrap();
    assert_eq!(empty.application_ratio, None);
    assert_eq!(empty.verified_outcome_ratio, None);
}

fn request(value: &Value) -> RecallFeedbackQualityRequest {
    RecallFeedbackQualityRequest {
        policy_revision: text(value, "policy_revision"),
        candidate_port_revision: text(value, "candidate_port_revision"),
        attribution_policy_revision: text(value, "attribution_policy_revision"),
        verifier_revision: text(value, "verifier_revision"),
        corpus_digest: text(value, "corpus_digest"),
        rows: value["rows"].as_array().unwrap().iter().map(row).collect(),
    }
}

fn row(value: &Value) -> RecallFeedbackRow {
    RecallFeedbackRow {
        exposure_id: text(value, "exposure_id"),
        selection_id: text(value, "selection_id"),
        record_id: text(value, "record_id"),
        revision_id: text(value, "revision_id"),
        applied: value["applied"].as_bool().unwrap(),
        outcome: value
            .get("outcome")
            .map(|outcome| match outcome.as_str().unwrap() {
                "verified" => RecallFeedbackOutcome::Verified,
                "falsified" => RecallFeedbackOutcome::Falsified,
                "neutral" => RecallFeedbackOutcome::Neutral,
                _ => panic!("unknown outcome"),
            }),
    }
}

fn ratio(value: &Value, key: &str) -> RecallQualityRatio {
    let values = value[key].as_array().unwrap();
    RecallQualityRatio {
        numerator: values[0].as_u64().unwrap(),
        denominator: values[1].as_u64().unwrap(),
    }
}

fn text(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap().to_owned()
}

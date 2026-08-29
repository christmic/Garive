use std::{fs, path::PathBuf};

use garive_memory::{
    evaluate_recall_quality, RecallQualityCase, RecallQualityIdentity, RecallQualityRatio,
};
use serde_json::Value;

fn fixture() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/fixtures/agent/memory-recall-quality-v1.json");
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn pinned_recall_quality_is_exact_and_replayable() {
    let root = fixture();
    let selection_fixture: Value = serde_json::from_str(
        &fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../spec/fixtures/agent/memory-hypothesis-lifecycle-v1.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(root["dataset_revision"], "synthetic-semantic-v1");
    for value in root["cases"].as_array().unwrap() {
        let selection = selection_fixture["recall_cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["name"] == value["selection_case"])
            .unwrap();
        let actual = value["selected"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item.as_str().unwrap().split_once(':').unwrap().0)
            .collect::<Vec<_>>();
        let expected = selection["expected_ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item.as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "{}", value["case_id"]);
    }
    let cases = root["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(case)
        .collect::<Vec<_>>();
    let summary = evaluate_recall_quality(&cases).unwrap();
    let expected = &root["expected_summary"];
    assert_eq!(summary.cases, expected["cases"].as_u64().unwrap());
    assert_eq!(summary.recall, Some(ratio(expected, "recall")));
    assert_eq!(summary.precision, Some(ratio(expected, "precision")));
    assert_eq!(
        summary.forbidden_admissions,
        expected["forbidden_admissions"].as_u64().unwrap()
    );
    assert_eq!(
        summary.replay_mismatches,
        expected["replay_mismatches"].as_u64().unwrap()
    );

    let mut invalid = cases[0].clone();
    invalid.selected.push(invalid.selected[0].clone());
    assert!(evaluate_recall_quality(&[invalid]).is_err());
}

fn case(value: &Value) -> RecallQualityCase {
    RecallQualityCase {
        case_id: value["case_id"].as_str().unwrap().into(),
        expected: identities(&value["expected"]),
        forbidden: identities(&value["forbidden"]),
        selected: identities(&value["selected"]),
        replay: identities(&value["replay"]),
    }
}

fn identities(value: &Value) -> Vec<RecallQualityIdentity> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|item| {
            let (record_id, revision_id) = item.as_str().unwrap().split_once(':').unwrap();
            RecallQualityIdentity {
                record_id: record_id.into(),
                revision_id: revision_id.into(),
            }
        })
        .collect()
}

fn ratio(value: &Value, prefix: &str) -> RecallQualityRatio {
    RecallQualityRatio {
        numerator: value[format!("{prefix}_numerator")].as_u64().unwrap(),
        denominator: value[format!("{prefix}_denominator")].as_u64().unwrap(),
    }
}

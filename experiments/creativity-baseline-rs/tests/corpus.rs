use garive_creativity_baseline::{load_creativity_corpus, CreativityBaselineErrorCode};
use garive_eval::CreativityTaskClass;
use serde_json::{json, Value};

const FIXTURE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../spec/fixtures/eval/creativity-corpus-v1.json"
));

#[test]
fn reference_corpus_is_complete_gold_separated_and_canonical() {
    let first = load_creativity_corpus(FIXTURE.as_bytes()).unwrap();
    let second = load_creativity_corpus(FIXTURE.as_bytes()).unwrap();
    assert_eq!(first, second);
    assert_eq!(first.canonical_digest.len(), 64);
    assert_eq!(first.tasks.len(), 4);
    assert_eq!(
        first
            .tasks
            .iter()
            .map(|task| task.task_class)
            .collect::<Vec<_>>(),
        CreativityTaskClass::ALL
    );
    assert!(first.tasks.iter().all(|task| {
        !task.generator_prompt.contains("required_constraints")
            && serde_json::from_str::<Value>(&task.evaluator_rubric_json).is_ok()
    }));
}

#[test]
fn duplicate_unknown_coverage_rubric_and_bounds_fail_closed() {
    let duplicated = FIXTURE.replacen("\"version\": 1,", "\"version\": 1,\n  \"version\": 1,", 1);
    assert_eq!(
        load_creativity_corpus(duplicated.as_bytes())
            .unwrap_err()
            .code(),
        CreativityBaselineErrorCode::InvalidDocument
    );

    let mutations: [fn(&mut Value); 5] = [
        |value: &mut Value| value["tasks"][0]["unknown"] = json!(true),
        |value: &mut Value| value["tasks"][3]["class"] = json!("design_alternatives"),
        |value: &mut Value| value["tasks"][0]["evaluator_rubric_json"] = json!("{"),
        |value: &mut Value| value["tasks"][0]["max_candidates"] = json!(1),
        |value: &mut Value| value["tasks"][0]["max_total_candidate_utf8_bytes"] = json!(2048),
    ];
    for mutation in mutations {
        let mut value: Value = serde_json::from_str(FIXTURE).unwrap();
        mutation(&mut value);
        assert!(load_creativity_corpus(&serde_json::to_vec(&value).unwrap()).is_err());
    }

    let mut duplicate_id: Value = serde_json::from_str(FIXTURE).unwrap();
    duplicate_id["tasks"][1]["task_id"] = duplicate_id["tasks"][0]["task_id"].clone();
    assert_eq!(
        load_creativity_corpus(&serde_json::to_vec(&duplicate_id).unwrap())
            .unwrap_err()
            .code(),
        CreativityBaselineErrorCode::InvalidCorpus
    );
}

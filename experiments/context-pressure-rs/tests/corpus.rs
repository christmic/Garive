use garive_context_pressure::{load_corpus, ContextPressureErrorCode};
use garive_eval::ContextWorkloadClass;
use serde_json::{json, Value};

const FIXTURE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../spec/fixtures/agent/context-pressure-corpus-v1.json"
));

#[test]
fn reference_corpus_is_complete_uncompressed_and_canonical() {
    let first = load_corpus(FIXTURE.as_bytes()).unwrap();
    let compact = serde_json::to_vec(&serde_json::from_str::<Value>(FIXTURE).unwrap()).unwrap();
    let second = load_corpus(&compact).unwrap();
    assert_eq!(first.canonical_digest, second.canonical_digest);
    assert_eq!(first.cases.len(), 4);
    assert_eq!(
        first
            .cases
            .iter()
            .map(|case| case.workload_class)
            .collect::<Vec<_>>(),
        ContextWorkloadClass::ALL
    );
}

#[test]
fn strict_schema_context_and_uncompressed_boundary_fail_closed() {
    let duplicate = FIXTURE.replacen("\"version\": 1,", "\"version\":1,\"version\":1,", 1);
    assert_eq!(
        load_corpus(duplicate.as_bytes()).unwrap_err().code(),
        ContextPressureErrorCode::InvalidDocument
    );
    let mut value: Value = serde_json::from_str(FIXTURE).unwrap();
    value["unknown"] = json!(true);
    assert_eq!(
        load_corpus(&serde_json::to_vec(&value).unwrap())
            .unwrap_err()
            .code(),
        ContextPressureErrorCode::InvalidDocument
    );
    value.as_object_mut().unwrap().remove("unknown");
    value["cases"][0]["request"]["max_items"] = json!(3);
    assert_eq!(
        load_corpus(&serde_json::to_vec(&value).unwrap())
            .unwrap_err()
            .code(),
        ContextPressureErrorCode::CompressedInput
    );
    value["cases"][0]["request"]["max_items"] = json!(16);
    value["cases"][0]["candidates"][1]["position"] = json!(1);
    assert_eq!(
        load_corpus(&serde_json::to_vec(&value).unwrap())
            .unwrap_err()
            .code(),
        ContextPressureErrorCode::InvalidContext
    );
}

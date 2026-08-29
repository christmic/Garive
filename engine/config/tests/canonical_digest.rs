use std::{fs, path::PathBuf};

use garive_config::digest_canonical_value;
use serde_json::Value;

#[test]
fn shared_digest_relations_hold() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/fixtures/agent/agent-definition-snapshot.json");
    let fixture: Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    for case in fixture["digest_relations"].as_array().unwrap() {
        let left: Value = serde_json::from_str(case["left_json"].as_str().unwrap()).unwrap();
        let right: Value = serde_json::from_str(case["right_json"].as_str().unwrap()).unwrap();
        let equal =
            digest_canonical_value(&left).unwrap() == digest_canonical_value(&right).unwrap();
        assert_eq!(equal, case["relation"] == "equal", "{}", case["name"]);
    }
}

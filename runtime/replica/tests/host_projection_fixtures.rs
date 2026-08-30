use std::collections::BTreeSet;

use serde_json::{Map, Value};

fn object<'a>(value: &'a Value, context: &str) -> &'a Map<String, Value> {
    value
        .as_object()
        .unwrap_or_else(|| panic!("{context} must be an object"))
}

fn array<'a>(root: &'a Value, key: &str) -> &'a [Value] {
    root.get(key)
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("{key} must be an array"))
}

fn exact_fields(value: &Value, context: &str, expected: &[&str]) {
    let actual = object(value, context)
        .keys()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let expected = expected.iter().copied().collect::<BTreeSet<_>>();
    assert_eq!(actual, expected, "{context} fields changed");
}

fn validate_cases(root: &Value, section: &str, fields: &[&str], names: &mut BTreeSet<String>) {
    for case in array(root, section) {
        exact_fields(case, section, fields);
        let name = case["name"]
            .as_str()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| panic!("{section} case name must be non-empty"));
        assert!(
            names.insert(name.to_owned()),
            "duplicate fixture case: {name}"
        );
    }
}

#[test]
fn host_read_model_fixture_is_strict_complete_and_uniquely_named() {
    let root: Value = serde_json::from_str(include_str!(
        "../../../spec/fixtures/host/host-read-model-v1.json"
    ))
    .unwrap();
    exact_fields(
        &root,
        "host-read-model-v1",
        &[
            "schema_version",
            "contract",
            "definition_cases",
            "session_page_cases",
            "session_view_cases",
            "timeline_cases",
            "cursor_cases",
            "failure_cases",
        ],
    );
    assert_eq!(root["schema_version"], 1);
    assert_eq!(root["contract"], "host-read-model-v1");
    let mut names = BTreeSet::new();
    validate_cases(
        &root,
        "definition_cases",
        &["name", "limit", "expected_ids", "error"],
        &mut names,
    );
    validate_cases(
        &root,
        "session_page_cases",
        &[
            "name",
            "limit",
            "before",
            "opened",
            "expected_ids",
            "has_next",
            "error",
        ],
        &mut names,
    );
    validate_cases(
        &root,
        "session_view_cases",
        &[
            "name",
            "prefix",
            "expected_state",
            "expected_turn_count",
            "error",
        ],
        &mut names,
    );
    validate_cases(
        &root,
        "timeline_cases",
        &[
            "name",
            "after_position",
            "limit",
            "prefix",
            "expected_states",
            "truncated",
            "error",
        ],
        &mut names,
    );
    validate_cases(
        &root,
        "cursor_cases",
        &["name", "scenario", "error"],
        &mut names,
    );
    validate_cases(
        &root,
        "failure_cases",
        &["name", "status", "code"],
        &mut names,
    );
    let errors = array(&root, "failure_cases")
        .iter()
        .map(|case| {
            (
                case["status"].as_u64().unwrap(),
                case["code"].as_str().unwrap(),
            )
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        errors,
        BTreeSet::from([
            (400, "invalid_request"),
            (404, "not_found"),
            (413, "read_bound_exceeded"),
            (500, "corrupt_state"),
            (503, "durability_unavailable"),
        ])
    );
}

#[test]
fn host_activity_fixture_is_strict_complete_and_uniquely_named() {
    let root: Value = serde_json::from_str(include_str!(
        "../../../spec/fixtures/host/host-agent-activity-v1.json"
    ))
    .unwrap();
    exact_fields(
        &root,
        "host-agent-activity-v1",
        &[
            "schema_version",
            "contract",
            "projection_cases",
            "timeline_cases",
            "reducer_cases",
            "bound_cases",
            "redaction_cases",
        ],
    );
    assert_eq!(root["schema_version"], 1);
    assert_eq!(root["contract"], "host-agent-activity-v1");
    let mut names = BTreeSet::new();
    validate_cases(
        &root,
        "projection_cases",
        &["name", "fact", "event", "state", "terminal", "safe_code"],
        &mut names,
    );
    validate_cases(
        &root,
        "timeline_cases",
        &["name", "facts", "expected_states", "error"],
        &mut names,
    );
    validate_cases(
        &root,
        "reducer_cases",
        &["name", "from", "fact", "to", "valid"],
        &mut names,
    );
    validate_cases(
        &root,
        "bound_cases",
        &["name", "bound", "error"],
        &mut names,
    );
    validate_cases(
        &root,
        "redaction_cases",
        &["name", "canary", "must_be_absent"],
        &mut names,
    );
    let mapped = array(&root, "projection_cases")
        .iter()
        .map(|case| case["fact"].as_str().unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        mapped,
        BTreeSet::from([
            "tool.preparation_rejected",
            "effect.prepared",
            "interaction.requested",
            "interaction.resolved",
            "interaction.cancelled",
            "effect.authorized",
            "effect.denied",
            "effect.started",
            "effect.receipt",
            "effect.completed",
            "effect.failed",
            "effect.uncertain",
            "effect.reconciled",
            "effect.observation",
        ])
    );
}

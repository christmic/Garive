use garive_memory::{
    retrieve_memory, ContentBinding, DurableFactReference, MemoryCommit, MemoryErrorCode,
    MemoryKind, MemoryProposal, MemoryPurpose, MemoryQuery, MemoryRecord, MemoryScope, MemoryScore,
    MemorySensitivity, MemoryState, MemoryStatus, MemoryTombstone,
};
use serde_json::Value;

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../spec/fixtures/agent/memory-capability-v1.json"
    ))
    .unwrap()
}

fn text(value: &Value, key: &str) -> String {
    value[key].as_str().expect("string").to_owned()
}

fn evidence(root: &Value) -> DurableFactReference {
    let value = &root["evidence"];
    DurableFactReference::new(
        text(value, "session_id"),
        value["position"].as_u64().unwrap(),
        text(value, "fact_id"),
        text(value, "payload_digest"),
    )
    .unwrap()
}

fn content(value: &Value) -> ContentBinding {
    if let Some(inline) = value.get("inline_utf8") {
        ContentBinding::inline(text(value, "digest"), inline.as_str().unwrap()).unwrap()
    } else {
        ContentBinding::referenced(text(value, "digest"), text(value, "reference")).unwrap()
    }
}

fn scope(value: &Value) -> MemoryScope {
    match value["kind"].as_str().unwrap() {
        "session" => MemoryScope::session(text(value, "owner_id")).unwrap(),
        "agent_instance" => MemoryScope::agent_instance(text(value, "owner_id")).unwrap(),
        "namespace" => MemoryScope::Namespace,
        other => panic!("unknown scope {other}"),
    }
}

fn kind(value: &str) -> MemoryKind {
    match value {
        "preference" => MemoryKind::Preference,
        "constraint" => MemoryKind::Constraint,
        "decision" => MemoryKind::Decision,
        "learned_fact" => MemoryKind::LearnedFact,
        "summary" => MemoryKind::Summary,
        other => panic!("unknown kind {other}"),
    }
}

fn sensitivity(value: &str) -> MemorySensitivity {
    match value {
        "ordinary" => MemorySensitivity::Ordinary,
        "restricted" => MemorySensitivity::Restricted,
        other => panic!("unknown sensitivity {other}"),
    }
}

fn record(root: &Value, value: &Value) -> MemoryRecord {
    MemoryRecord::new(
        text(value, "record_id"),
        text(value, "revision_id"),
        text(value, "namespace_id"),
        scope(&value["scope"]),
        kind(value["kind"].as_str().unwrap()),
        content(&value["content"]),
        vec![evidence(root)],
        match value["status"].as_str().unwrap() {
            "active" => MemoryStatus::Active,
            "superseded" => MemoryStatus::Superseded,
            "tombstoned" => MemoryStatus::Tombstoned,
            other => panic!("unknown status {other}"),
        },
        sensitivity(value["sensitivity"].as_str().unwrap()),
        value["confidence_basis_points"].as_u64().unwrap() as u16,
        value["valid_from_position"].as_u64().unwrap(),
        value
            .get("supersedes_revision_id")
            .and_then(Value::as_str)
            .map(str::to_owned),
        value
            .get("expires_at_utc")
            .and_then(Value::as_str)
            .map(str::to_owned),
    )
    .unwrap()
}

fn query(value: &Value) -> MemoryQuery {
    MemoryQuery::new(
        text(value, "query_id"),
        text(value, "namespace_id"),
        value["allowed_scopes"]
            .as_array()
            .unwrap()
            .iter()
            .map(scope)
            .collect(),
        match value["purpose"].as_str().unwrap() {
            "context" => MemoryPurpose::Context,
            "planning" => MemoryPurpose::Planning,
            "conflict_check" => MemoryPurpose::ConflictCheck,
            other => panic!("unknown purpose {other}"),
        },
        text(value, "retriever_revision"),
        content(&value["query"]),
        value["through_position"].as_u64().unwrap(),
        text(value, "as_of_utc"),
        value["max_results"].as_u64().unwrap() as u32,
        value["max_total_bytes"].as_u64().unwrap(),
        value["include_restricted"].as_bool().unwrap(),
        value
            .get("restricted_grant_digest")
            .and_then(Value::as_str)
            .map(str::to_owned),
    )
    .unwrap()
}

fn proposal(root: &Value, value: &Value) -> MemoryProposal {
    MemoryProposal::new(
        text(value, "proposal_id"),
        text(value, "namespace_id"),
        scope(&value["scope"]),
        kind(value["kind"].as_str().unwrap()),
        content(&value["content"]),
        vec![evidence(root)],
        sensitivity(value["sensitivity"].as_str().unwrap()),
        value["confidence_basis_points"].as_u64().unwrap() as u16,
        value
            .get("expected_active_revision_id")
            .and_then(Value::as_str)
            .map(str::to_owned),
    )
    .unwrap()
}

fn commit(value: &Value) -> MemoryCommit {
    MemoryCommit::new(
        text(value, "record_id"),
        text(value, "revision_id"),
        text(value, "retention_policy_digest"),
        value["valid_from_position"].as_u64().unwrap(),
        value
            .get("expires_at_utc")
            .and_then(Value::as_str)
            .map(str::to_owned),
        value
            .get("supersedes_revision_id")
            .and_then(Value::as_str)
            .map(str::to_owned),
    )
    .unwrap()
}

#[test]
fn shared_queries_enforce_order_visibility_time_and_prefix_bounds() {
    let root = fixture();
    let records: Vec<_> = root["records"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| record(&root, value))
        .collect();
    let scores: Vec<_> = root["scores"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| {
            MemoryScore::new(
                text(value, "record_id"),
                text(value, "revision_id"),
                value["relevance_basis_points"].as_u64().unwrap() as u16,
                value["content_byte_length"].as_u64().unwrap(),
            )
            .unwrap()
        })
        .collect();
    for value in root["queries"].as_array().unwrap() {
        let query = query(value);
        if let Some(expected) = value.get("expected_query_digest") {
            assert_eq!(
                query.query_digest().unwrap(),
                expected.as_str().unwrap(),
                "{}",
                value["name"]
            );
        }
        let result = retrieve_memory(&records, &scores, &query).unwrap();
        let ids: Vec<_> = result.matches.iter().map(|item| item.record_id()).collect();
        let expected: Vec<_> = value["expected"]["record_ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item.as_str().unwrap())
            .collect();
        assert_eq!(ids, expected, "{}", value["name"]);
        assert_eq!(
            result.truncated,
            value["expected"]["truncated"].as_bool().unwrap()
        );
    }
}

#[test]
fn shared_write_cases_apply_atomically() {
    let root = fixture();
    let records: Vec<_> = root["records"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| record(&root, value))
        .collect();
    for value in root["write_cases"].as_array().unwrap() {
        let mut state = MemoryState::new(records.clone()).unwrap();
        let before = state.clone();
        let result = state.commit(
            &proposal(&root, &value["proposal"]),
            &commit(&value["commit"]),
        );
        match value["expected"].as_str().unwrap() {
            "committed" => {
                let outcome = result.unwrap();
                assert_eq!(outcome.record.status(), MemoryStatus::Active);
                assert_eq!(state.revisions().len(), before.revisions().len() + 1);
            }
            "revision_conflict" => {
                assert_eq!(
                    result.unwrap_err().code(),
                    MemoryErrorCode::RevisionConflict
                );
                assert_eq!(state, before);
            }
            other => panic!("unknown expected {other}"),
        }
    }
}

#[test]
fn invalid_authority_size_evidence_and_tombstone_shapes_fail_closed() {
    assert_eq!(
        MemoryQuery::new(
            "q",
            "ns",
            vec![MemoryScope::Namespace],
            MemoryPurpose::Context,
            "r",
            ContentBinding::from_inline("q"),
            0,
            "2026-08-29T00:00:00Z",
            1,
            1,
            true,
            None,
        )
        .unwrap_err()
        .code(),
        MemoryErrorCode::InvalidMemory,
    );
    let root = fixture();
    let active = record(&root, &root["records"][1]);
    let mismatch = MemoryScore::new(active.record_id(), active.revision_id(), 1, 99).unwrap();
    assert_eq!(
        retrieve_memory(&[active], &[mismatch], &query(&root["queries"][0]))
            .unwrap_err()
            .code(),
        MemoryErrorCode::InvalidMemory,
    );
    let proof = evidence(&root);
    assert_eq!(
        MemoryProposal::new(
            "p",
            "ns",
            MemoryScope::Namespace,
            MemoryKind::Summary,
            ContentBinding::from_inline("x"),
            vec![proof.clone(), proof],
            MemorySensitivity::Ordinary,
            1,
            None,
        )
        .unwrap_err()
        .code(),
        MemoryErrorCode::InvalidMemory,
    );
    let old = record(&root, &root["records"][3]);
    let mut state = MemoryState::new(vec![old.clone()]).unwrap();
    assert_eq!(
        state
            .tombstone(&MemoryTombstone {
                record_id: old.record_id().into(),
                revision_id: old.revision_id().into()
            })
            .unwrap_err()
            .code(),
        MemoryErrorCode::RevisionConflict,
    );
}

use std::{fs, path::PathBuf};

use garive_multiagent::{
    authorize_delegation, complete_delegation_result, release_delegation_budget,
    CancellationPolicy, ChildRequirement, ContentBinding, DelegationAllowance, DelegationBudget,
    DelegationConsumption, DelegationErrorCode, DelegationIntent, DelegationResultContext,
    DelegationUsage, FactReference, TokenUsageEvidence,
};
use serde_json::Value;

fn fixture() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/fixtures/agent/multi-agent-delegation-v1.json");
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn budget(value: &Value) -> DelegationBudget {
    DelegationBudget {
        max_child_turns: value["max_child_turns"].as_u64().unwrap(),
        max_child_executions: value["max_child_executions"].as_u64().unwrap(),
        max_iterations: value["max_iterations"].as_u64().unwrap(),
        max_input_tokens: value["max_input_tokens"].as_u64().unwrap(),
        max_output_tokens: value["max_output_tokens"].as_u64().unwrap(),
        deadline_budget_ms: value["deadline_budget_ms"].as_u64().unwrap(),
        max_depth: value["max_depth"].as_u64().unwrap(),
        max_objective_bytes: value["max_objective_bytes"].as_u64().unwrap(),
        max_input_evidence: value["max_input_evidence"].as_u64().unwrap(),
        max_result_schema_bytes: value["max_result_schema_bytes"].as_u64().unwrap(),
        max_result_bytes: value["max_result_bytes"].as_u64().unwrap(),
        max_result_evidence: value["max_result_evidence"].as_u64().unwrap(),
    }
}

fn intent(root: &Value) -> DelegationIntent {
    let value = &root["intent"];
    let child = &value["child_requirement"];
    let objective = &value["objective"];
    let schema = &value["result_schema"];
    let evidence = &value["input_evidence"][0];
    DelegationIntent::new(
        value["delegation_id"].as_str().unwrap(),
        value["parent_agent_instance_id"].as_str().unwrap(),
        value["parent_turn_id"].as_str().unwrap(),
        value["parent_execution_id"].as_str().unwrap(),
        ChildRequirement::definition(
            child["definition_id"].as_str().unwrap(),
            child["definition_revision"].as_str().unwrap(),
        )
        .unwrap(),
        ContentBinding::from_inline(objective["inline_utf8"].as_str().unwrap()),
        vec![FactReference::new(
            evidence["session_id"].as_str().unwrap(),
            evidence["position"].as_u64().unwrap(),
            evidence["fact_id"].as_str().unwrap(),
            evidence["payload_digest"].as_str().unwrap(),
        )
        .unwrap()],
        ContentBinding::from_inline(schema["inline_utf8"].as_str().unwrap()),
        budget(&value["budget"]),
        CancellationPolicy::CancelWithParent,
        value["through_position"].as_u64().unwrap(),
    )
    .unwrap()
}

fn allowance(multiplier: u64, budget: &DelegationBudget) -> DelegationAllowance {
    DelegationAllowance {
        remaining_child_turns: budget.max_child_turns * multiplier,
        remaining_child_executions: budget.max_child_executions * multiplier,
        remaining_iterations: budget.max_iterations * multiplier,
        remaining_input_tokens: budget.max_input_tokens * multiplier,
        remaining_output_tokens: budget.max_output_tokens * multiplier,
        remaining_elapsed_ms: budget.deadline_budget_ms * multiplier,
        max_depth: budget.max_depth,
        max_objective_bytes: budget.max_objective_bytes,
        max_input_evidence: budget.max_input_evidence,
        max_result_schema_bytes: budget.max_result_schema_bytes,
        max_result_bytes: budget.max_result_bytes,
        max_result_evidence: budget.max_result_evidence,
    }
}

#[test]
fn shared_intent_digest_failures_and_budget_settlement_are_exact() {
    let root = fixture();
    let intent = intent(&root);
    assert_eq!(
        intent.intent_digest().unwrap(),
        root["intent"]["expected_intent_digest"]
    );
    let original = allowance(2, intent.budget());
    let authorization =
        authorize_delegation(&intent, "grant-1", "authority-1", 0, 0, &original).unwrap();
    let result = &root["completed_result"];
    let usage = DelegationUsage {
        input_tokens: TokenUsageEvidence::Known { value: 10 },
        output_tokens: TokenUsageEvidence::Known { value: 5 },
    };
    let consumption = DelegationConsumption {
        child_turns: 1,
        child_executions: 2,
        completed_iterations: 4,
        elapsed_ms: 1_200,
    };
    let completed = complete_delegation_result(
        &intent,
        DelegationResultContext {
            result_id: "result-1".into(),
            delegation_id: "delegation-1".into(),
            grant_id: "grant-1".into(),
            child_agent_instance_id: "child-agent".into(),
            child_turn_id: "child-turn".into(),
            child_snapshot_digest: "c".repeat(64),
            usage,
            consumption,
        },
        ContentBinding::from_inline(result["content"]["inline_utf8"].as_str().unwrap()),
        result["content"]["inline_utf8"].as_str().unwrap(),
        Vec::new(),
    )
    .unwrap();
    let settlement = completed.settlement();
    assert_eq!(settlement.charged.input_tokens, 10);
    assert_eq!(settlement.released.input_tokens, 90);
    assert_eq!(
        completed.result_binding().unwrap().digest(),
        result["expected_result_digest"].as_str().unwrap()
    );
    let released =
        release_delegation_budget(&authorization.remaining, settlement, &original).unwrap();
    assert_eq!(
        released.remaining_input_tokens,
        original.remaining_input_tokens - 10
    );

    let expected: Vec<_> = root["failure_codes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect();
    let codes = [
        DelegationErrorCode::InvalidDelegation,
        DelegationErrorCode::ChildNotFound,
        DelegationErrorCode::ChildRevisionMismatch,
        DelegationErrorCode::AuthorityDenied,
        DelegationErrorCode::BudgetExhausted,
        DelegationErrorCode::BudgetOverflow,
        DelegationErrorCode::DepthExceeded,
        DelegationErrorCode::ConcurrencyExceeded,
        DelegationErrorCode::ResultSchemaMismatch,
        DelegationErrorCode::DelegationConflict,
        DelegationErrorCode::ChildStateCorrupt,
        DelegationErrorCode::DurabilityFailure,
        DelegationErrorCode::CorruptDelegationState,
    ];
    assert_eq!(
        codes.map(DelegationErrorCode::wire_name).as_slice(),
        expected
    );
}

#[test]
fn bounds_depth_concurrency_schema_and_unknown_usage_fail_closed() {
    let root = fixture();
    let intent = intent(&root);
    let exact = allowance(1, intent.budget());
    assert_eq!(
        authorize_delegation(&intent, "grant", "authority", 2, 0, &exact)
            .unwrap_err()
            .code(),
        DelegationErrorCode::DepthExceeded
    );
    assert_eq!(
        authorize_delegation(&intent, "grant", "authority", 0, 1, &exact)
            .unwrap_err()
            .code(),
        DelegationErrorCode::ConcurrencyExceeded
    );
    let exhausted = allowance(0, intent.budget());
    assert_eq!(
        authorize_delegation(&intent, "grant", "authority", 0, 0, &exhausted)
            .unwrap_err()
            .code(),
        DelegationErrorCode::BudgetExhausted
    );

    let invalid = complete_delegation_result(
        &intent,
        DelegationResultContext {
            result_id: "result".into(),
            delegation_id: "delegation-1".into(),
            grant_id: "grant".into(),
            child_agent_instance_id: "child".into(),
            child_turn_id: "turn".into(),
            child_snapshot_digest: "c".repeat(64),
            usage: DelegationUsage {
                input_tokens: TokenUsageEvidence::Unknown,
                output_tokens: TokenUsageEvidence::Unknown,
            },
            consumption: DelegationConsumption {
                child_turns: 1,
                child_executions: 1,
                completed_iterations: 1,
                elapsed_ms: 1,
            },
        },
        ContentBinding::from_inline("[]"),
        "[]",
        Vec::new(),
    )
    .unwrap_err();
    assert_eq!(invalid.code(), DelegationErrorCode::ResultSchemaMismatch);
}

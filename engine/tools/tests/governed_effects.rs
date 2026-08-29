use std::{fs, path::PathBuf};

use garive_tools::{
    reduce_preparation_failure, AuthorizationVerdict, DispatchAttemptId, EffectReceipt,
    EffectState, ExecutionCapability, ExecutionFact, ExecutionRequirements, GovernedAction,
    GovernedEffect, GovernedFailureCode, GovernedToolResult, GrantId, InteractionId,
    InteractionKind, InteractionRequest, InteractionResolution, InvocationGrant, ReceiptId,
    ReplayClass, SuspensionRequirement, TerminalClassification, ToolCatalog, ToolDefinition,
    ToolFeedback, ToolIntent, ToolInvocationId,
};
use serde_json::Value;

fn fixture() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/fixtures/agent/governed-effects.json");
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn capability(value: &str) -> ExecutionCapability {
    match value {
        "filesystem_read" => ExecutionCapability::FilesystemRead,
        "filesystem_write" => ExecutionCapability::FilesystemWrite,
        "process" => ExecutionCapability::Process,
        "network" => ExecutionCapability::Network,
        other => panic!("unknown capability: {other}"),
    }
}

fn requirements(value: &Value) -> ExecutionRequirements {
    ExecutionRequirements::new(
        value["capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| capability(item.as_str().unwrap())),
        value["max_duration_ms"].as_u64().unwrap(),
        value["max_output_bytes"].as_u64().unwrap(),
    )
    .unwrap()
}

fn catalog(fixture: &Value) -> ToolCatalog {
    let expected = &fixture["prepared_call"];
    let definition = ToolDefinition::new(
        expected["tool_name"].as_str().unwrap(),
        expected["tool_revision"].as_str().unwrap(),
        "Read one admitted file.",
        serde_json::json!({
            "$schema":"https://json-schema.org/draft/2020-12/schema",
            "type":"object",
            "properties":{"path":{"type":"string","minLength":1}},
            "required":["path"],
            "additionalProperties":false
        }),
        requirements(&expected["requirements"]),
        ReplayClass::ReadOnly,
    )
    .unwrap();
    ToolCatalog::new([definition]).unwrap()
}

fn prepared(fixture: &Value) -> garive_tools::PreparedToolCall {
    let expected = &fixture["prepared_call"];
    let call = catalog(fixture)
        .prepare(&ToolIntent::new(
            expected["model_call_id"].as_str().unwrap(),
            expected["tool_name"].as_str().unwrap(),
            expected["normalized_arguments"].as_str().unwrap(),
        ))
        .unwrap();
    assert_eq!(call.input_digest(), expected["input_digest"]);
    call
}

fn grant(fixture: &Value, name: &str) -> InvocationGrant {
    let value = &fixture["grants"][name];
    InvocationGrant::new(
        GrantId::new(value["grant_id"].as_str().unwrap()).unwrap(),
        ToolInvocationId::new(value["invocation_id"].as_str().unwrap()).unwrap(),
        value["prepared_digest"].as_str().unwrap(),
        value["tool_name"].as_str().unwrap(),
        value["tool_revision"].as_str().unwrap(),
        requirements(&value["granted_requirements"]),
        value["constraints_digest"].as_str().unwrap(),
        value["authority_revision"].as_str().unwrap(),
    )
    .unwrap()
}

fn interaction(fixture: &Value) -> InteractionRequest {
    let value = &fixture["interaction"];
    InteractionRequest {
        interaction_id: InteractionId::new(value["interaction_id"].as_str().unwrap()).unwrap(),
        invocation_id: ToolInvocationId::new(value["invocation_id"].as_str().unwrap()).unwrap(),
        prepared_digest: value["prepared_digest"].as_str().unwrap().to_owned(),
        kind: InteractionKind::Approval,
        prompt: value["prompt"].clone(),
        response_schema: value["response_schema"].clone(),
        expiry_policy: value["expiry_policy"].as_str().unwrap().to_owned(),
    }
}

fn receipt(fixture: &Value, name: &str) -> EffectReceipt {
    let value = &fixture["receipts"][name];
    EffectReceipt {
        receipt_id: ReceiptId::new(value["receipt_id"].as_str().unwrap()).unwrap(),
        invocation_id: ToolInvocationId::new(value["invocation_id"].as_str().unwrap()).unwrap(),
        prepared_digest: value["prepared_digest"].as_str().unwrap().to_owned(),
        grant_id: GrantId::new(value["grant_id"].as_str().unwrap()).unwrap(),
        executor_id: value["executor_id"].as_str().unwrap().to_owned(),
        executor_revision: value["executor_revision"].as_str().unwrap().to_owned(),
        terminal_classification: match value["terminal_classification"].as_str().unwrap() {
            "completed" => TerminalClassification::Completed,
            "failed" => TerminalClassification::Failed,
            other => panic!("unknown terminal: {other}"),
        },
        result_digest: value["result_digest"].as_str().unwrap().to_owned(),
    }
}

fn action_name(action: &GovernedAction) -> &'static str {
    match action {
        GovernedAction::Authorize => "authorize",
        GovernedAction::Dispatch(_) => "dispatch",
        GovernedAction::Observation(observation) => {
            match observation.model_envelope()["status"].as_str().unwrap() {
                "succeeded" => "observation_succeeded",
                "rejected" => "observation_rejected",
                "failed" => "observation_failed",
                other => panic!("unknown observation: {other}"),
            }
        }
        GovernedAction::Suspend(SuspensionRequirement::Interaction(request)) => {
            match request.kind {
                InteractionKind::Approval => "suspend_approval",
                InteractionKind::ExternalInput => "suspend_external_input",
            }
        }
        GovernedAction::Suspend(SuspensionRequirement::OperatorReconciliation { .. }) => {
            "suspend_reconciliation"
        }
        GovernedAction::Fail(failure) => match failure.code {
            GovernedFailureCode::GrantMismatch => "fail_grant_mismatch",
            GovernedFailureCode::RequirementUnsupported => "fail_requirement_unsupported",
            GovernedFailureCode::InvocationConflict => "fail_invocation_conflict",
            GovernedFailureCode::InteractionConflict => "fail_interaction_conflict",
            GovernedFailureCode::CorruptRecoveryState => "fail_corrupt_recovery_state",
            GovernedFailureCode::InvalidModelOutput => "fail_invalid_model_output",
        },
        GovernedAction::None => "none",
    }
}

fn state_name(state: EffectState) -> &'static str {
    match state {
        EffectState::Prepared => "prepared",
        EffectState::Denied => "denied",
        EffectState::Replaced => "replaced",
        EffectState::AwaitingInteraction => "awaiting_interaction",
        EffectState::Authorized => "authorized",
        EffectState::Started => "started",
        EffectState::Completed => "completed",
        EffectState::Failed => "failed",
        EffectState::Uncertain => "uncertain",
    }
}

#[test]
fn shared_governed_scenarios_match() {
    let fixture = fixture();
    for case in fixture["scenarios"].as_array().unwrap() {
        let invocation = ToolInvocationId::new(fixture["invocation_id"].as_str().unwrap()).unwrap();
        let (mut reducer, first) = GovernedEffect::new(invocation, prepared(&fixture));
        let mut actions = vec![first];
        for operation in case["operations"].as_array().unwrap() {
            let action = match operation["kind"].as_str().unwrap() {
                "approve" => reducer.apply_authorization(AuthorizationVerdict::Approve(grant(
                    &fixture,
                    operation["grant"].as_str().unwrap(),
                ))),
                "deny" => reducer.apply_authorization(AuthorizationVerdict::Deny {
                    code: operation["code"].as_str().unwrap().to_owned(),
                    details: operation["details"].as_str().map(str::to_owned),
                }),
                "replacement_required" => {
                    reducer.apply_authorization(AuthorizationVerdict::ReplacementRequired)
                }
                "interaction_required" => reducer.apply_authorization(
                    AuthorizationVerdict::InteractionRequired(interaction(&fixture)),
                ),
                "interaction_resolved" => {
                    let request = interaction(&fixture);
                    reducer.apply_interaction(InteractionResolution::Resolved {
                        interaction_id: request.interaction_id,
                        invocation_id: request.invocation_id,
                        prepared_digest: request.prepared_digest,
                        response: operation["response"].clone(),
                    })
                }
                "interaction_cancelled" => {
                    let request = interaction(&fixture);
                    reducer.apply_interaction(InteractionResolution::Cancelled {
                        interaction_id: request.interaction_id,
                        invocation_id: request.invocation_id,
                        prepared_digest: request.prepared_digest,
                    })
                }
                "started" => reducer.apply_execution(ExecutionFact::Started(
                    DispatchAttemptId::new(operation["dispatch_attempt_id"].as_str().unwrap())
                        .unwrap(),
                )),
                "completed" => reducer.apply_execution(ExecutionFact::Completed {
                    receipt: operation["receipt"]
                        .as_str()
                        .map(|name| receipt(&fixture, name)),
                    content: operation["content"].clone(),
                    truncated: operation["truncated"].as_bool().unwrap(),
                }),
                "failed" => reducer.apply_execution(ExecutionFact::Failed {
                    receipt: operation["receipt"]
                        .as_str()
                        .map(|name| receipt(&fixture, name)),
                    code: operation["code"].as_str().unwrap().to_owned(),
                    details: operation["details"].as_str().map(str::to_owned),
                    partial: operation.get("partial").cloned(),
                }),
                "uncertain" => reducer.apply_execution(ExecutionFact::Uncertain {
                    evidence: operation["evidence"].as_str().unwrap().to_owned(),
                }),
                "unsupported" => reducer.apply_execution(ExecutionFact::Unsupported {
                    requirement: operation["requirement"].as_str().unwrap().to_owned(),
                }),
                other => panic!("unknown operation: {other}"),
            };
            actions.push(action);
        }
        let expected = &case["expected"];
        let names: Vec<_> = actions.iter().map(action_name).collect();
        assert_eq!(
            names,
            expected["actions"]
                .as_array()
                .unwrap()
                .iter()
                .map(|item| item.as_str().unwrap())
                .collect::<Vec<_>>(),
            "{}",
            case["name"]
        );
        assert_eq!(
            actions
                .iter()
                .filter(|action| matches!(action, GovernedAction::Dispatch(_)))
                .count() as u64,
            expected["execution_command_count"].as_u64().unwrap(),
            "{}",
            case["name"]
        );
        assert_eq!(state_name(reducer.state()), expected["final_state"]);
        if let Some(expected_observation) = expected.get("observation") {
            let observation = actions.iter().find_map(|action| match action {
                GovernedAction::Observation(value) => Some(value.model_envelope()),
                _ => None,
            });
            assert_eq!(
                observation.as_ref(),
                Some(expected_observation),
                "{}",
                case["name"]
            );
        }
    }
}

#[test]
fn governance_identities_reject_empty_values() {
    assert!(ToolInvocationId::new("").is_err());
    assert!(GrantId::new("").is_err());
    assert!(InteractionId::new("").is_err());
    assert!(ReceiptId::new("").is_err());
    assert!(DispatchAttemptId::new("").is_err());
}

#[test]
fn shared_preparation_failures_reduce_safely() {
    let fixture = fixture();
    for case in fixture["preparation_cases"].as_array().unwrap() {
        let input = &case["intent"];
        let intent = ToolIntent::new(
            input["model_call_id"].as_str().unwrap(),
            input["tool_name"].as_str().unwrap(),
            input["arguments_json"].as_str().unwrap(),
        );
        let error = catalog(&fixture).prepare(&intent).unwrap_err();
        let result = reduce_preparation_failure(&intent, &error);
        match (&result, case["expected"]["result"].as_str().unwrap()) {
            (
                GovernedToolResult::Observation(ToolFeedback::PreparationRejected(feedback)),
                "observation",
            ) => {
                assert_eq!(feedback.code, error.code());
                assert_eq!(
                    feedback.failure_paths,
                    case["expected"]["failure_paths"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|item| item.as_str().unwrap().to_owned())
                        .collect::<Vec<_>>()
                );
            }
            (GovernedToolResult::Fail(failure), "fail") => {
                assert_eq!(failure.code, GovernedFailureCode::InvalidModelOutput);
            }
            _ => panic!("{} produced the wrong result", case["name"]),
        }
    }
}

use std::{fs, path::PathBuf};

use garive_tools::{
    AuthorizationVerdict, DispatchAttemptId, EffectReceipt, ExecutionCapability, ExecutionFact,
    ExecutionRequirements, GovernedAction, GovernedEffect, GrantId, InvocationGrant, ReceiptId,
    ReplayClass, TerminalClassification, ToolCatalog, ToolDefinition, ToolIntent, ToolInvocationId,
};
use serde_json::{json, Value};

fn fixture() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/fixtures/agent/governed-effects.json");
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn requirements(duration: u64, output: u64) -> ExecutionRequirements {
    ExecutionRequirements::new([ExecutionCapability::FilesystemRead], duration, output).unwrap()
}

fn catalog() -> ToolCatalog {
    ToolCatalog::new([ToolDefinition::new(
        "read_file",
        "1",
        "Read one admitted file.",
        json!({
            "$schema":"https://json-schema.org/draft/2020-12/schema",
            "type":"object",
            "properties":{"path":{"type":"string","minLength":1}},
            "required":["path"],
            "additionalProperties":false
        }),
        requirements(5000, 4096),
        ReplayClass::ReadOnly,
    )
    .unwrap()])
    .unwrap()
}

fn prepared() -> garive_tools::PreparedToolCall {
    catalog()
        .prepare(&ToolIntent::new(
            "call-a",
            "read_file",
            r#"{"path":"src/main.rs"}"#,
        ))
        .unwrap()
}

fn grant(call: &garive_tools::PreparedToolCall) -> InvocationGrant {
    InvocationGrant::new(
        GrantId::new("grant-1").unwrap(),
        ToolInvocationId::new("invocation-1").unwrap(),
        call.input_digest(),
        call.tool_name(),
        call.tool_revision(),
        requirements(4000, 2048),
        "constraints-v1",
        "policy-7",
    )
    .unwrap()
}

#[derive(Default)]
struct FakeRuntime {
    committed: Vec<&'static str>,
    external_dispatches: usize,
}

impl FakeRuntime {
    fn commit(&mut self, fact: &'static str) {
        self.committed.push(fact);
    }

    fn execute_success(&mut self) {
        let call = prepared();
        let invocation = ToolInvocationId::new("invocation-1").unwrap();
        self.commit("effect.prepared");
        let (mut reducer, action) = GovernedEffect::new(invocation.clone(), call.clone());
        assert!(matches!(action, GovernedAction::Authorize));

        let grant = grant(&call);
        self.commit("effect.authorized");
        assert!(matches!(
            reducer.apply_authorization(AuthorizationVerdict::Approve(grant.clone())),
            GovernedAction::Dispatch(_)
        ));

        self.commit("effect.started");
        self.external_dispatches += 1;
        assert!(matches!(
            reducer.apply_execution(ExecutionFact::Started(
                DispatchAttemptId::new("dispatch-1").unwrap()
            )),
            GovernedAction::None
        ));

        self.commit("effect.receipt");
        let receipt = EffectReceipt {
            receipt_id: ReceiptId::new("receipt-1").unwrap(),
            invocation_id: invocation,
            prepared_digest: call.input_digest().to_owned(),
            grant_id: grant.grant_id,
            executor_id: "local.read".to_owned(),
            executor_revision: "4".to_owned(),
            terminal_classification: TerminalClassification::Completed,
            result_digest: "result-v1".to_owned(),
        };
        let action = reducer.apply_execution(ExecutionFact::Completed {
            receipt: Some(receipt),
            content: json!({"text":"hello"}),
            truncated: false,
        });
        self.commit("effect.completed");
        assert!(matches!(action, GovernedAction::Observation(_)));
        self.commit("effect.observation");
    }
}

#[test]
fn fake_runtime_commits_every_boundary_before_action() {
    let fixture = fixture();
    let mut runtime = FakeRuntime::default();
    runtime.execute_success();
    assert_eq!(runtime.external_dispatches, 1);
    assert_eq!(
        runtime.committed,
        fixture["durable_order"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item.as_str().unwrap())
            .collect::<Vec<_>>()
    );
}

#[test]
fn invalid_and_unapproved_calls_never_cross_dispatch() {
    let invalid = catalog().prepare(&ToolIntent::new("call", "read_file", "{}"));
    assert!(invalid.is_err());

    for verdict in [
        AuthorizationVerdict::Deny {
            code: "actor_denied".to_owned(),
            details: None,
        },
        AuthorizationVerdict::ReplacementRequired,
    ] {
        let call = prepared();
        let (mut reducer, _) =
            GovernedEffect::new(ToolInvocationId::new("invocation-1").unwrap(), call);
        assert!(!matches!(
            reducer.apply_authorization(verdict),
            GovernedAction::Dispatch(_)
        ));
    }
}

use garive_multiagent::{
    CollaborationToolCatalogue, COLLECT_DELEGATIONS_TOOL, DELEGATE_TOOL, FORK_SELF_TOOL,
    MESSAGE_AGENT_TOOL,
};
use garive_tools::{AccessMode, AccessNamespace, ReplayClass, ToolIntent};

#[test]
fn catalogue_prepares_all_agent_collaboration_commands() {
    let catalogue = CollaborationToolCatalogue::new("collaboration.policy.v1").unwrap();
    let cases = [
        (
            MESSAGE_AGENT_TOOL,
            r#"{"recipient":"Birch","text":"review the ledger"}"#,
            ReplayClass::Idempotent,
            AccessMode::Write,
        ),
        (
            DELEGATE_TOOL,
            r#"{"assignee":{"kind":"named","agent_name":"Birch"},"objective":"review the ledger"}"#,
            ReplayClass::Idempotent,
            AccessMode::Write,
        ),
        (
            FORK_SELF_TOOL,
            r#"{"objective":"explore another approach","branch_name":"alternative"}"#,
            ReplayClass::Idempotent,
            AccessMode::Write,
        ),
        (
            COLLECT_DELEGATIONS_TOOL,
            r#"{"max_results":8}"#,
            ReplayClass::ReadOnly,
            AccessMode::Read,
        ),
    ];
    for (index, (name, arguments, replay, mode)) in cases.into_iter().enumerate() {
        let prepared = catalogue
            .prepare(&ToolIntent::new(format!("call-{index}"), name, arguments))
            .unwrap();
        assert_eq!(prepared.tool_name(), name);
        assert_eq!(prepared.contract_version(), 3);
        assert_eq!(prepared.replay_class(), replay);
        let [access] = prepared.invocation_accesses().unwrap().values() else {
            panic!("one Runtime lane is required")
        };
        assert_eq!(access.namespace(), AccessNamespace::Runtime);
        assert_eq!(access.mode(), mode);
    }
}

#[test]
fn message_schema_forbids_model_supplied_actor_identity() {
    let catalogue = CollaborationToolCatalogue::new("collaboration.policy.v1").unwrap();
    let error = catalogue
        .prepare(&ToolIntent::new(
            "call-forged",
            MESSAGE_AGENT_TOOL,
            r#"{"from_agent_instance_id":"agent-forged","recipient":"Birch","text":"fake"}"#,
        ))
        .unwrap_err();
    assert!(!error.failures().is_empty());
}

#[test]
fn catalogue_rejects_invalid_selector_shapes() {
    let catalogue = CollaborationToolCatalogue::new("collaboration.policy.v1").unwrap();
    for arguments in [
        r#"{"assignee":{"kind":"named"},"objective":"work"}"#,
        r#"{"assignee":{"kind":"anonymous","definition_id":""},"objective":"work"}"#,
        r#"{"assignee":{"kind":"unknown"},"objective":"work"}"#,
    ] {
        assert!(catalogue
            .prepare(&ToolIntent::new("call-invalid", DELEGATE_TOOL, arguments))
            .is_err());
    }
}

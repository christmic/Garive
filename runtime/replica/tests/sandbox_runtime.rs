use garive_runtime::{
    preflight_sandbox, SafetyDecisionV1, SafetyDisposition, SandboxBindingV1, SandboxPreflightError,
};
use garive_tools::{
    AccessMode, AccessNamespace, AccessPolicyEntry, ExecutionCapability, ExecutionRequirements,
    GrantId, InvocationAccessSet, InvocationGrant, PreparationError, ReplayClass, ResourceAccess,
    SandboxControl, SandboxRequirementsV1, ToolAccessPolicyV1, ToolAccessResolver, ToolCatalog,
    ToolDefinition, ToolIntent, ToolInvocationId,
};
use serde_json::{json, Value};

struct Resolver;

impl ToolAccessResolver for Resolver {
    fn revision(&self) -> &str {
        "resolver-v1"
    }

    fn resolve(&self, arguments: &Value) -> Result<InvocationAccessSet, PreparationError> {
        InvocationAccessSet::new([ResourceAccess::new(
            AccessNamespace::Filesystem,
            arguments["path"].as_str().unwrap(),
            AccessMode::Read,
        )?])
    }
}

#[test]
fn exact_allow_and_stricter_binding_prepare_without_dispatch() {
    let fixture = fixture("src", 4);
    let execution = preflight_sandbox(
        &fixture.invocation,
        &fixture.prepared,
        &fixture.grant,
        &fixture.decision,
        &fixture.binding,
        "dispatch-1",
    )
    .unwrap();
    assert_eq!(execution.executor_id, "executor");
    assert_eq!(execution.executor_revision, "executor-v1");
    assert_eq!(execution.dispatch_attempt_id, "dispatch-1");
}

#[test]
fn denied_decision_never_reaches_executor_preflight() {
    let fixture = fixture("src", 4);
    let denied = SafetyDecisionV1::new(
        "decision-denied",
        SafetyDisposition::Deny,
        fixture.invocation.clone(),
        fixture.prepared.input_digest(),
        None,
        "policy-v1",
        Some("safety_denied".into()),
    )
    .unwrap();
    assert_eq!(
        preflight_sandbox(
            &fixture.invocation,
            &fixture.prepared,
            &fixture.grant,
            &denied,
            &fixture.binding,
            "dispatch-1",
        ),
        Err(SandboxPreflightError::DecisionNotAllowed)
    );
}

#[test]
fn stale_policy_and_out_of_scope_access_fail_closed() {
    let stale = fixture("src", 4);
    let stale_binding = binding("src", 4, "policy-v2");
    assert_eq!(
        preflight_sandbox(
            &stale.invocation,
            &stale.prepared,
            &stale.grant,
            &stale.decision,
            &stale_binding,
            "dispatch-1",
        ),
        Err(SandboxPreflightError::BindingStale)
    );

    let outside = fixture("other", 4);
    assert_eq!(
        preflight_sandbox(
            &outside.invocation,
            &outside.prepared,
            &outside.grant,
            &outside.decision,
            &outside.binding,
            "dispatch-1",
        ),
        Err(SandboxPreflightError::ScopeMismatch)
    );
}

#[test]
fn weaker_executor_limit_is_unsupported() {
    let fixture = fixture("src", 16);
    assert_eq!(
        preflight_sandbox(
            &fixture.invocation,
            &fixture.prepared,
            &fixture.grant,
            &fixture.decision,
            &fixture.binding,
            "dispatch-1",
        ),
        Err(SandboxPreflightError::EnforcementUnsupported)
    );
}

struct Fixture {
    invocation: ToolInvocationId,
    prepared: garive_tools::PreparedToolCall,
    grant: InvocationGrant,
    decision: SafetyDecisionV1,
    binding: SandboxBindingV1,
}

fn fixture(binding_root: &str, binding_open_files: u32) -> Fixture {
    let prepared = ToolCatalog::new([definition()])
        .unwrap()
        .prepare_v3(
            &ToolIntent::new("call", "read", r#"{"path":"src/lib.rs"}"#),
            &Resolver,
        )
        .unwrap();
    let invocation = ToolInvocationId::new("invocation").unwrap();
    let grant = InvocationGrant::new(
        GrantId::new("grant").unwrap(),
        invocation.clone(),
        prepared.input_digest(),
        prepared.tool_name(),
        prepared.tool_revision(),
        prepared.requirements().clone(),
        "constraints-v1",
        "policy-v1",
    )
    .unwrap();
    let decision = SafetyDecisionV1::new(
        "decision",
        SafetyDisposition::Allow,
        invocation.clone(),
        prepared.input_digest(),
        Some("constraints-v1".into()),
        "policy-v1",
        None,
    )
    .unwrap();
    Fixture {
        invocation,
        prepared,
        grant,
        decision,
        binding: binding(binding_root, binding_open_files, "policy-v1"),
    }
}

fn definition() -> ToolDefinition {
    ToolDefinition::new_v3(
        "read",
        "read-v3",
        "Read one file",
        json!({
            "type":"object",
            "properties":{"path":{"type":"string"}},
            "required":["path"],
            "additionalProperties":false
        }),
        ExecutionRequirements::new([ExecutionCapability::FilesystemRead], 1_000, 4_096).unwrap(),
        ReplayClass::ReadOnly,
        access_policy("src"),
        "resolver-v1",
        sandbox(8),
    )
    .unwrap()
}

fn binding(root: &str, max_open_files: u32, policy: &str) -> SandboxBindingV1 {
    SandboxBindingV1::new(
        "binding",
        "workspace-capability",
        "executor",
        "executor-v1",
        policy,
        access_policy(root),
        sandbox(max_open_files),
    )
    .unwrap()
}

fn access_policy(root: &str) -> ToolAccessPolicyV1 {
    ToolAccessPolicyV1::new(
        "access-v1",
        [AccessPolicyEntry::new(root, [AccessMode::Read]).unwrap()],
        [],
        [],
        [],
        1,
        4_096,
    )
    .unwrap()
}

fn sandbox(max_open_files: u32) -> SandboxRequirementsV1 {
    SandboxRequirementsV1::new(
        [ExecutionCapability::FilesystemRead],
        [
            SandboxControl::FilesystemScope,
            SandboxControl::SymlinkContainment,
            SandboxControl::ResourceLimits,
        ],
        None,
        max_open_files,
    )
    .unwrap()
}

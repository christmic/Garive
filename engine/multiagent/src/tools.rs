use serde_json::{json, Value};

use garive_tools::{
    AccessMode, AccessNamespace, AccessPolicyEntry, ExecutionRequirements, InvocationAccessSet,
    PreparationError, PreparedToolCall, ReplayClass, ResourceAccess, SandboxControl,
    SandboxRequirementsV1, ToolAccessPolicyV1, ToolAccessResolver, ToolCatalog, ToolDefinition,
    ToolIntent,
};

/// Stable tool name for an addressed or broadcast peer message.
pub const MESSAGE_AGENT_TOOL: &str = "garive.collaboration.message_agent";
/// Stable tool name for named or anonymous task delegation.
pub const DELEGATE_TOOL: &str = "garive.collaboration.delegate";
/// Stable tool name for a task-scoped fork of the active Agent.
pub const FORK_SELF_TOOL: &str = "garive.collaboration.fork_self";
/// Stable tool name for reading the active Agent's delegation results.
pub const COLLECT_DELEGATIONS_TOOL: &str = "garive.collaboration.collect_delegations";

/// Immutable revision shared by the first autonomous collaboration tools.
pub const COLLABORATION_TOOL_REVISION: &str = "1";
/// Pure Runtime-lane resolver revision.
pub const COLLABORATION_ACCESS_RESOLVER_REVISION: &str = "garive.collaboration.access.v1";

const MESSAGE_LANE: &str = "session_messages";
const DELEGATION_LANE: &str = "session_delegations";
const MAX_TEXT_BYTES: u64 = 65_536;
const MAX_RESULTS: u64 = 64;
const MAX_RESULT_BYTES: u64 = 262_144;

/// Exact four-tool catalogue frozen into an autonomous Agent snapshot.
#[derive(Clone, Debug)]
pub struct CollaborationToolCatalogue {
    definitions: Vec<ToolDefinition>,
    catalogue: ToolCatalog,
}

impl CollaborationToolCatalogue {
    /// Constructs the catalogue from one explicit Runtime policy revision.
    pub fn new(policy_revision: impl Into<String>) -> Result<Self, PreparationError> {
        let policy_revision = policy_revision.into();
        let mut definitions = vec![
            definition(
                MESSAGE_AGENT_TOOL,
                "Send a message to one named Session peer, or omit recipient to broadcast.",
                json!({"type":"object","properties":{"recipient":{"type":"string","minLength":1,"maxLength":64},"text":{"type":"string","minLength":1,"maxLength":MAX_TEXT_BYTES}},"required":["text"],"additionalProperties":false}),
                ReplayClass::Idempotent,
                MESSAGE_LANE,
                AccessMode::Write,
                &policy_revision,
            )?,
            definition(
                DELEGATE_TOOL,
                "Delegate one Notify task to a named peer or an anonymous Agent.",
                json!({"type":"object","properties":{"assignee":{"oneOf":[{"type":"object","properties":{"kind":{"const":"named"},"agent_name":{"type":"string","minLength":1,"maxLength":64}},"required":["kind","agent_name"],"additionalProperties":false},{"type":"object","properties":{"kind":{"const":"anonymous"},"definition_id":{"type":"string","minLength":1,"maxLength":128}},"required":["kind","definition_id"],"additionalProperties":false}]},"objective":{"type":"string","minLength":1,"maxLength":MAX_TEXT_BYTES}},"required":["assignee","objective"],"additionalProperties":false}),
                ReplayClass::Idempotent,
                DELEGATION_LANE,
                AccessMode::Write,
                &policy_revision,
            )?,
            definition(
                FORK_SELF_TOOL,
                "Fork this Agent for one independent Notify task.",
                json!({"type":"object","properties":{"objective":{"type":"string","minLength":1,"maxLength":MAX_TEXT_BYTES}},"required":["objective"],"additionalProperties":false}),
                ReplayClass::Idempotent,
                DELEGATION_LANE,
                AccessMode::Write,
                &policy_revision,
            )?,
            definition(
                COLLECT_DELEGATIONS_TOOL,
                "Read bounded delegation states and delivered results for this Agent.",
                json!({"type":"object","properties":{"max_results":{"type":"integer","minimum":1,"maximum":MAX_RESULTS}},"required":["max_results"],"additionalProperties":false}),
                ReplayClass::ReadOnly,
                DELEGATION_LANE,
                AccessMode::Read,
                &policy_revision,
            )?,
        ];
        definitions.sort_by(|left, right| left.name().cmp(right.name()));
        let catalogue = ToolCatalog::new(definitions.clone())?;
        Ok(Self {
            definitions,
            catalogue,
        })
    }

    /// Returns definitions in stable tool-name order.
    pub fn definitions(&self) -> &[ToolDefinition] {
        &self.definitions
    }

    /// Validates and prepares one model-originated collaboration intent.
    pub fn prepare(&self, intent: &ToolIntent) -> Result<PreparedToolCall, PreparationError> {
        self.catalogue.prepare_v3(
            intent,
            &CollaborationAccessResolver {
                tool_name: intent.tool_name(),
            },
        )
    }
}

struct CollaborationAccessResolver<'a> {
    tool_name: &'a str,
}

impl ToolAccessResolver for CollaborationAccessResolver<'_> {
    fn revision(&self) -> &str {
        COLLABORATION_ACCESS_RESOLVER_REVISION
    }

    fn resolve(&self, _: &Value) -> Result<InvocationAccessSet, PreparationError> {
        let (lane, mode) = match self.tool_name {
            MESSAGE_AGENT_TOOL => (MESSAGE_LANE, AccessMode::Write),
            DELEGATE_TOOL | FORK_SELF_TOOL => (DELEGATION_LANE, AccessMode::Write),
            COLLECT_DELEGATIONS_TOOL => (DELEGATION_LANE, AccessMode::Read),
            _ => return InvocationAccessSet::new([]),
        };
        InvocationAccessSet::new([ResourceAccess::new(AccessNamespace::Runtime, lane, mode)?])
    }
}

fn definition(
    name: &str,
    description: &str,
    schema: Value,
    replay: ReplayClass,
    lane: &str,
    mode: AccessMode,
    policy_revision: &str,
) -> Result<ToolDefinition, PreparationError> {
    let requirements = ExecutionRequirements::new([], 5_000, MAX_RESULT_BYTES)?;
    let access = ToolAccessPolicyV1::new(
        policy_revision,
        [],
        [],
        [],
        [AccessPolicyEntry::new(lane, [mode])?],
        1,
        MAX_RESULT_BYTES,
    )?;
    let sandbox = SandboxRequirementsV1::new([], [SandboxControl::ResourceLimits], None, 1)?;
    ToolDefinition::new_v3(
        name,
        COLLABORATION_TOOL_REVISION,
        description,
        schema,
        requirements,
        replay,
        access,
        COLLABORATION_ACCESS_RESOLVER_REVISION,
        sandbox,
    )
}

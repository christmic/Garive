//! Exact T1 built-in definitions and pure resource resolvers.

use std::collections::BTreeSet;

use serde_json::{json, Value};

use crate::{
    t1_patch_targets, AccessMode, AccessNamespace, AccessPolicyEntry, ExecutionCapability,
    ExecutionRequirements, InvocationAccessSet, PreparationError, PreparationErrorCode,
    PreparedToolCall, ReplayClass, ResourceAccess, SandboxControl, SandboxRequirementsV1,
    ToolAccessPolicyV1, ToolAccessResolver, ToolCatalog, ToolDefinition, ToolIntent,
};

/// Exact immutable revision shared by every T1 definition.
pub const T1_TOOL_REVISION: &str = "1";
/// Pure resolver revision for the exact T1 argument contract.
pub const T1_ACCESS_RESOLVER_REVISION: &str = "garive.t1.access.v1";
/// Exact read-text tool name.
pub const T1_READ_TEXT: &str = "garive.workspace.read_text";
/// Exact directory-list tool name.
pub const T1_LIST: &str = "garive.workspace.list";
/// Exact literal-search tool name.
pub const T1_SEARCH_TEXT: &str = "garive.workspace.search_text";
/// Exact create-only UTF-8 write tool name.
pub const T1_WRITE_TEXT: &str = "garive.workspace.write_text";
/// Exact journaled-patch tool name.
pub const T1_APPLY_PATCH: &str = "garive.workspace.apply_patch";
/// Exact bounded-process tool name.
pub const T1_PROCESS_RUN: &str = "garive.process.run";

const MAX_FILE_BYTES: u64 = 1_048_576;
const MAX_RESULT_BYTES: u64 = 2_097_152;
const MAX_PATCH_BYTES: u64 = 1_048_576;
const MAX_EXPECTED_FILES: u64 = 128;
const MAX_PROCESS_ARGUMENTS: u64 = 256;
const MAX_ARGUMENT_BYTES: u64 = 32_768;
const MAX_PROCESS_DURATION_MS: u64 = 300_000;
const MAX_PROCESS_OUTPUT_BYTES: u64 = 1_048_576;
const MAX_TRAVERSAL_NODES: u64 = 10_000;

/// Frozen six-tool catalogue for one effective Agent snapshot.
#[derive(Clone, Debug)]
pub struct BuiltinT1Catalogue {
    definitions: Vec<ToolDefinition>,
    catalog: ToolCatalog,
}

impl BuiltinT1Catalogue {
    /// Constructs definitions from an explicit policy revision and process lane set.
    pub fn new(
        policy_revision: impl Into<String>,
        process_lanes: impl IntoIterator<Item = impl Into<String>>,
    ) -> Result<Self, PreparationError> {
        let policy_revision = policy_revision.into();
        let lanes = process_lanes
            .into_iter()
            .map(Into::into)
            .collect::<Vec<_>>();
        let mut definitions = vec![
            read_definition(&policy_revision)?,
            list_definition(&policy_revision)?,
            search_definition(&policy_revision)?,
            write_definition(&policy_revision)?,
            patch_definition(&policy_revision)?,
            process_definition(&policy_revision, &lanes)?,
        ];
        definitions.sort_by(|left, right| left.name().cmp(right.name()));
        let catalog = ToolCatalog::new(definitions.clone())?;
        Ok(Self {
            definitions,
            catalog,
        })
    }

    /// Returns the exact definitions to freeze in an Agent capability snapshot.
    pub fn definitions(&self) -> &[ToolDefinition] {
        &self.definitions
    }

    /// Validates and prepares one admitted T1 intent through its exact resolver.
    pub fn prepare(&self, intent: &ToolIntent) -> Result<PreparedToolCall, PreparationError> {
        self.catalog.prepare_v3(
            intent,
            &BuiltinT1Resolver {
                tool_name: intent.tool_name(),
            },
        )
    }
}

struct BuiltinT1Resolver<'a> {
    tool_name: &'a str,
}

impl ToolAccessResolver for BuiltinT1Resolver<'_> {
    fn revision(&self) -> &str {
        T1_ACCESS_RESOLVER_REVISION
    }

    fn resolve(&self, arguments: &Value) -> Result<InvocationAccessSet, PreparationError> {
        match self.tool_name {
            T1_READ_TEXT => one_file(arguments, AccessMode::Read, false),
            T1_WRITE_TEXT => one_file(arguments, AccessMode::Write, false),
            T1_LIST | T1_SEARCH_TEXT => one_file(arguments, AccessMode::Read, true),
            T1_APPLY_PATCH => patch_accesses(arguments),
            T1_PROCESS_RUN => process_accesses(arguments),
            _ => Err(access_error()),
        }
    }
}

fn one_file(
    arguments: &Value,
    mode: AccessMode,
    root_allowed: bool,
) -> Result<InvocationAccessSet, PreparationError> {
    let path = text(arguments, "path")?;
    if !root_allowed && path == "." {
        return Err(access_error());
    }
    InvocationAccessSet::new([ResourceAccess::new(
        AccessNamespace::Filesystem,
        path,
        mode,
    )?])
}

fn process_accesses(arguments: &Value) -> Result<InvocationAccessSet, PreparationError> {
    let workspace_mode = match text(arguments, "workspace_mode")? {
        "read" => AccessMode::Read,
        "write" => AccessMode::Write,
        _ => return Err(access_error()),
    };
    InvocationAccessSet::new([
        ResourceAccess::new(
            AccessNamespace::Process,
            text(arguments, "lane")?,
            AccessMode::Exclusive,
        )?,
        ResourceAccess::new(
            AccessNamespace::Filesystem,
            text(arguments, "working_directory")?,
            workspace_mode,
        )?,
    ])
}

fn patch_accesses(arguments: &Value) -> Result<InvocationAccessSet, PreparationError> {
    let targets = patch_targets(text(arguments, "patch")?)?;
    let expected = arguments
        .get("expected_files")
        .and_then(Value::as_array)
        .ok_or_else(access_error)?;
    let mut declared = BTreeSet::new();
    for value in expected {
        let path = text(value, "path")?;
        let digest = text(value, "before_digest")?;
        if path == "."
            || digest.len() != 64
            || !digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            || !declared.insert(path.to_owned())
        {
            return Err(access_error());
        }
    }
    if targets != declared {
        return Err(access_error());
    }
    InvocationAccessSet::new(
        targets
            .into_iter()
            .map(|path| ResourceAccess::new(AccessNamespace::Filesystem, path, AccessMode::Write))
            .collect::<Result<Vec<_>, _>>()?,
    )
}

fn patch_targets(patch: &str) -> Result<BTreeSet<String>, PreparationError> {
    let targets = t1_patch_targets(patch).map_err(|_| access_error())?;
    for path in &targets {
        ResourceAccess::new(AccessNamespace::Filesystem, path, AccessMode::Write)?;
    }
    Ok(targets)
}

fn read_definition(policy_revision: &str) -> Result<ToolDefinition, PreparationError> {
    file_definition(
        T1_READ_TEXT,
        "Read one bounded UTF-8 workspace file.",
        json!({"type":"object","properties":{"path":{"type":"string","minLength":1,"maxLength":4096},"max_bytes":{"type":"integer","minimum":1,"maximum":MAX_FILE_BYTES}},"required":["path","max_bytes"],"additionalProperties":false}),
        ReplayClass::ReadOnly,
        [ExecutionCapability::FilesystemRead],
        [AccessMode::Read],
        policy_revision,
        5_000,
        1,
    )
}

fn list_definition(policy_revision: &str) -> Result<ToolDefinition, PreparationError> {
    file_definition(
        T1_LIST,
        "List one workspace directory without following links.",
        json!({"type":"object","properties":{"path":{"type":"string","minLength":1,"maxLength":4096},"max_entries":{"type":"integer","minimum":1,"maximum":4096},"include_hidden":{"type":"boolean"},"max_nodes":{"type":"integer","minimum":1,"maximum":MAX_TRAVERSAL_NODES}},"required":["path","max_entries","include_hidden","max_nodes"],"additionalProperties":false}),
        ReplayClass::ReadOnly,
        [ExecutionCapability::FilesystemRead],
        [AccessMode::Read],
        policy_revision,
        5_000,
        1,
    )
}

fn search_definition(policy_revision: &str) -> Result<ToolDefinition, PreparationError> {
    file_definition(
        T1_SEARCH_TEXT,
        "Search bounded workspace text for one literal query.",
        json!({"type":"object","properties":{"path":{"type":"string","minLength":1,"maxLength":4096},"query":{"type":"string","minLength":1,"maxLength":4096},"case_sensitive":{"type":"boolean"},"max_matches":{"type":"integer","minimum":1,"maximum":4096},"max_file_bytes":{"type":"integer","minimum":1,"maximum":MAX_FILE_BYTES},"max_nodes":{"type":"integer","minimum":1,"maximum":MAX_TRAVERSAL_NODES}},"required":["path","query","case_sensitive","max_matches","max_file_bytes","max_nodes"],"additionalProperties":false}),
        ReplayClass::ReadOnly,
        [ExecutionCapability::FilesystemRead],
        [AccessMode::Read],
        policy_revision,
        30_000,
        1,
    )
}

fn write_definition(policy_revision: &str) -> Result<ToolDefinition, PreparationError> {
    file_definition(
        T1_WRITE_TEXT,
        "Create one bounded UTF-8 workspace file without overwriting.",
        json!({"type":"object","properties":{"path":{"type":"string","minLength":1,"maxLength":4096},"text":{"type":"string","maxLength":MAX_FILE_BYTES}},"required":["path","text"],"additionalProperties":false}),
        ReplayClass::NeverReplay,
        [ExecutionCapability::FilesystemWrite],
        [AccessMode::Write],
        policy_revision,
        5_000,
        1,
    )
}

fn patch_definition(policy_revision: &str) -> Result<ToolDefinition, PreparationError> {
    file_definition(
        T1_APPLY_PATCH,
        "Apply a standard unified diff or Garive patch to existing workspace files. Every target must include its digest from read_text.",
        json!({"type":"object","properties":{"patch":{"type":"string","description":"A standard unified diff with --- a/path, +++ b/path and @@ range @@ headers, or a Garive *** Begin Patch block.","minLength":1,"maxLength":MAX_PATCH_BYTES},"expected_files":{"type":"array","description":"Every patched path exactly once, bound to the SHA-256 content_digest returned by read_text.","minItems":1,"maxItems":MAX_EXPECTED_FILES,"items":{"type":"object","properties":{"path":{"type":"string","minLength":1,"maxLength":4096},"before_digest":{"type":"string","minLength":64,"maxLength":64}},"required":["path","before_digest"],"additionalProperties":false}}},"required":["patch","expected_files"],"additionalProperties":false}),
        ReplayClass::ReceiptRecoverable,
        [
            ExecutionCapability::FilesystemRead,
            ExecutionCapability::FilesystemWrite,
        ],
        [AccessMode::Read, AccessMode::Write],
        policy_revision,
        30_000,
        MAX_EXPECTED_FILES as usize,
    )
}

#[allow(clippy::too_many_arguments)]
fn file_definition<const C: usize, const M: usize>(
    name: &str,
    description: &str,
    schema: Value,
    replay: ReplayClass,
    capabilities: [ExecutionCapability; C],
    modes: [AccessMode; M],
    policy_revision: &str,
    max_duration_ms: u64,
    max_accesses: usize,
) -> Result<ToolDefinition, PreparationError> {
    let requirements = ExecutionRequirements::new(capabilities, max_duration_ms, MAX_RESULT_BYTES)?;
    ToolDefinition::new_v3(
        name,
        T1_TOOL_REVISION,
        description,
        schema,
        requirements.clone(),
        replay,
        ToolAccessPolicyV1::new(
            policy_revision,
            [AccessPolicyEntry::new(".", modes)?],
            [],
            [],
            [],
            max_accesses,
            MAX_RESULT_BYTES,
        )?,
        T1_ACCESS_RESOLVER_REVISION,
        filesystem_sandbox(requirements.capabilities())?,
    )
}

fn process_definition(
    policy_revision: &str,
    lanes: &[String],
) -> Result<ToolDefinition, PreparationError> {
    let capabilities = [
        ExecutionCapability::FilesystemRead,
        ExecutionCapability::FilesystemWrite,
        ExecutionCapability::Process,
    ];
    let requirements =
        ExecutionRequirements::new(capabilities, MAX_PROCESS_DURATION_MS, MAX_RESULT_BYTES)?;
    let process_entries = lanes
        .iter()
        .map(|lane| AccessPolicyEntry::new(lane, [AccessMode::Exclusive]))
        .collect::<Result<Vec<_>, _>>()?;
    ToolDefinition::new_v3(
        T1_PROCESS_RUN,
        T1_TOOL_REVISION,
        "Run one configured executable lane without shell parsing.",
        json!({"type":"object","properties":{"lane":{"type":"string","minLength":1,"maxLength":256},"argv":{"type":"array","minItems":1,"maxItems":MAX_PROCESS_ARGUMENTS,"items":{"type":"string","minLength":1,"maxLength":MAX_ARGUMENT_BYTES}},"working_directory":{"type":"string","minLength":1,"maxLength":4096},"workspace_mode":{"type":"string","enum":["read","write"]},"max_output_bytes":{"type":"integer","minimum":1,"maximum":MAX_PROCESS_OUTPUT_BYTES},"timeout_ms":{"type":"integer","minimum":1,"maximum":MAX_PROCESS_DURATION_MS}},"required":["lane","argv","working_directory","workspace_mode","max_output_bytes","timeout_ms"],"additionalProperties":false}),
        requirements.clone(),
        ReplayClass::NeverReplay,
        ToolAccessPolicyV1::new(
            policy_revision,
            [AccessPolicyEntry::new(
                ".",
                [AccessMode::Read, AccessMode::Write],
            )?],
            process_entries,
            [],
            [],
            2,
            MAX_RESULT_BYTES,
        )?,
        T1_ACCESS_RESOLVER_REVISION,
        SandboxRequirementsV1::new(
            requirements.capabilities(),
            [
                SandboxControl::FilesystemScope,
                SandboxControl::SymlinkContainment,
                SandboxControl::ProcessContainment,
                SandboxControl::StructuredArguments,
                SandboxControl::EnvironmentAllowlist,
                SandboxControl::ResourceLimits,
            ],
            Some(16),
            64,
        )?,
    )
}

fn filesystem_sandbox(
    capabilities: impl IntoIterator<Item = ExecutionCapability>,
) -> Result<SandboxRequirementsV1, PreparationError> {
    SandboxRequirementsV1::new(
        capabilities,
        [
            SandboxControl::FilesystemScope,
            SandboxControl::SymlinkContainment,
            SandboxControl::ResourceLimits,
        ],
        None,
        64,
    )
}

fn text<'a>(value: &'a Value, field: &str) -> Result<&'a str, PreparationError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(access_error)
}

fn access_error() -> PreparationError {
    PreparationError::new(PreparationErrorCode::EffectAccessInvalid)
}

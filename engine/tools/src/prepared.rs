use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::schema::{parse_arguments, validate_arguments, validate_definition};

/// Stable failure classification for C4 preparation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreparationErrorCode {
    /// Model correlation identity is empty.
    InvalidModelCallId,
    /// Proposed tool name is empty.
    InvalidToolName,
    /// The frozen catalog does not admit the proposed tool.
    ToolNotAdmitted,
    /// Argument text is malformed, trailing, or contains duplicate keys.
    InvalidArgumentsJson,
    /// Parsed arguments do not satisfy the exact tool schema.
    ArgumentsSchemaMismatch,
    /// A tool definition or execution requirement is invalid.
    InvalidToolDefinition,
    /// The schema contains a keyword outside Portable Tool Schema v1.
    UnsupportedSchemaKeyword,
    /// A value cannot be represented by the admitted JCS surface.
    NonCanonicalValue,
}

/// Deterministic JSON Schema assertion failure.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SchemaFailure {
    instance_path: String,
    schema_path: String,
    keyword: String,
}

impl SchemaFailure {
    pub(crate) fn new(instance_path: &str, schema_path: &str, keyword: &str) -> Self {
        Self {
            instance_path: instance_path.to_owned(),
            schema_path: schema_path.to_owned(),
            keyword: keyword.to_owned(),
        }
    }
    /// Returns the RFC 6901 path into the rejected instance.
    pub fn instance_path(&self) -> &str {
        &self.instance_path
    }
    /// Returns the RFC 6901 path into the exact schema.
    pub fn schema_path(&self) -> &str {
        &self.schema_path
    }
    /// Returns the stable failing keyword.
    pub fn keyword(&self) -> &str {
        &self.keyword
    }
}

/// Typed C4 construction or preparation failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparationError {
    code: PreparationErrorCode,
    failures: Vec<SchemaFailure>,
}

impl PreparationError {
    pub(crate) const fn new(code: PreparationErrorCode) -> Self {
        Self {
            code,
            failures: Vec::new(),
        }
    }
    pub(crate) const fn with_failures(
        code: PreparationErrorCode,
        failures: Vec<SchemaFailure>,
    ) -> Self {
        Self { code, failures }
    }
    /// Returns the stable failure classification.
    pub const fn code(&self) -> PreparationErrorCode {
        self.code
    }
    /// Returns deterministic schema failures, empty for non-schema failures.
    pub fn failures(&self) -> &[SchemaFailure] {
        &self.failures
    }
}

/// Neutral executor capability declared by one exact tool definition.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionCapability {
    /// Read from an admitted filesystem surface.
    FilesystemRead,
    /// Mutate an admitted filesystem surface.
    FilesystemWrite,
    /// Start a bounded process.
    Process,
    /// Access an admitted network surface.
    Network,
}

/// Immutable executor requirements carried by a Prepared Call.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ExecutionRequirements {
    capabilities: BTreeSet<ExecutionCapability>,
    max_duration_ms: u64,
    max_output_bytes: u64,
}

impl ExecutionRequirements {
    /// Validates non-zero limits and unique capabilities.
    pub fn new(
        capabilities: impl IntoIterator<Item = ExecutionCapability>,
        max_duration_ms: u64,
        max_output_bytes: u64,
    ) -> Result<Self, PreparationError> {
        let values: Vec<_> = capabilities.into_iter().collect();
        let unique: BTreeSet<_> = values.iter().copied().collect();
        if max_duration_ms == 0 || max_output_bytes == 0 || unique.len() != values.len() {
            return Err(PreparationError::new(
                PreparationErrorCode::InvalidToolDefinition,
            ));
        }
        Ok(Self {
            capabilities: unique,
            max_duration_ms,
            max_output_bytes,
        })
    }
    /// Returns capabilities in their canonical enum order.
    pub fn capabilities(&self) -> impl ExactSizeIterator<Item = ExecutionCapability> + '_ {
        self.capabilities.iter().copied()
    }
    /// Returns the maximum admitted execution duration.
    pub const fn max_duration_ms(&self) -> u64 {
        self.max_duration_ms
    }
    /// Returns the maximum admitted model-visible output size.
    pub const fn max_output_bytes(&self) -> u64 {
        self.max_output_bytes
    }
}

/// Recovery safety declaration that Runtime must independently prove.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplayClass {
    /// Read-only operation eligible for a proven same-ID retry.
    ReadOnly,
    /// Executor supports a proven idempotency identity.
    Idempotent,
    /// Executor recovers from a committed receipt or journal.
    ReceiptRecoverable,
    /// An uncertain started operation always requires reconciliation.
    NeverReplay,
}

/// Exact immutable definition admitted to one execution snapshot.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ToolDefinition {
    name: String,
    revision: String,
    description: String,
    input_schema: Value,
    requirements: ExecutionRequirements,
    replay_class: ReplayClass,
}

impl ToolDefinition {
    /// Validates and constructs one Portable Tool Schema v1 definition.
    pub fn new(
        name: impl Into<String>,
        revision: impl Into<String>,
        description: impl Into<String>,
        input_schema: Value,
        requirements: ExecutionRequirements,
        replay_class: ReplayClass,
    ) -> Result<Self, PreparationError> {
        let (name, revision, description) = (name.into(), revision.into(), description.into());
        if name.is_empty() || revision.is_empty() || description.is_empty() {
            return Err(PreparationError::new(
                PreparationErrorCode::InvalidToolDefinition,
            ));
        }
        validate_definition(&input_schema)?;
        if replay_class == ReplayClass::ReadOnly
            && requirements
                .capabilities()
                .any(|capability| capability != ExecutionCapability::FilesystemRead)
        {
            return Err(PreparationError::new(
                PreparationErrorCode::InvalidToolDefinition,
            ));
        }
        Ok(Self {
            name,
            revision,
            description,
            input_schema,
            requirements,
            replay_class,
        })
    }
    /// Returns the admitted provider-neutral name.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Returns the immutable definition revision.
    pub fn revision(&self) -> &str {
        &self.revision
    }
    /// Returns the model-visible description.
    pub fn description(&self) -> &str {
        &self.description
    }
    /// Returns the validated Portable Tool Schema v1 value.
    pub const fn input_schema(&self) -> &Value {
        &self.input_schema
    }
    /// Returns the declared execution requirements.
    pub const fn requirements(&self) -> &ExecutionRequirements {
        &self.requirements
    }
    /// Returns the declared recovery class.
    pub const fn replay_class(&self) -> ReplayClass {
        self.replay_class
    }
}

/// Untrusted model proposal supplied to C4.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolIntent {
    model_call_id: String,
    tool_name: String,
    arguments_json: String,
}

impl ToolIntent {
    /// Constructs an untrusted proposal; validation happens during preparation.
    pub fn new(
        model_call_id: impl Into<String>,
        tool_name: impl Into<String>,
        arguments_json: impl Into<String>,
    ) -> Self {
        Self {
            model_call_id: model_call_id.into(),
            tool_name: tool_name.into(),
            arguments_json: arguments_json.into(),
        }
    }
}

/// Frozen exact-name catalog for one Kernel Execution.
#[derive(Clone, Debug)]
pub struct ToolCatalog {
    definitions: BTreeMap<String, ToolDefinition>,
}

impl ToolCatalog {
    /// Rejects duplicate names and constructs an immutable catalog.
    pub fn new(
        definitions: impl IntoIterator<Item = ToolDefinition>,
    ) -> Result<Self, PreparationError> {
        let mut catalog = BTreeMap::new();
        for definition in definitions {
            let name = definition.name.clone();
            if catalog.insert(name, definition).is_some() {
                return Err(PreparationError::new(
                    PreparationErrorCode::InvalidToolDefinition,
                ));
            }
        }
        Ok(Self {
            definitions: catalog,
        })
    }
    /// Validates one untrusted intent and returns an immutable authority-free call.
    pub fn prepare(&self, intent: &ToolIntent) -> Result<PreparedToolCall, PreparationError> {
        if intent.model_call_id.is_empty() {
            return Err(PreparationError::new(
                PreparationErrorCode::InvalidModelCallId,
            ));
        }
        if intent.tool_name.is_empty() {
            return Err(PreparationError::new(PreparationErrorCode::InvalidToolName));
        }
        let definition = self
            .definitions
            .get(&intent.tool_name)
            .ok_or_else(|| PreparationError::new(PreparationErrorCode::ToolNotAdmitted))?;
        let arguments = parse_arguments(&intent.arguments_json)?;
        let failures = validate_arguments(&definition.input_schema, &arguments);
        if !failures.is_empty() {
            return Err(PreparationError::with_failures(
                PreparationErrorCode::ArgumentsSchemaMismatch,
                failures,
            ));
        }
        PreparedToolCall::from_parts(intent, definition, arguments)
    }
}

/// Immutable validated call carrying no invocation identity or authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedToolCall {
    model_call_id: String,
    tool_name: String,
    tool_revision: String,
    normalized_arguments: String,
    input_digest: String,
    requirements: ExecutionRequirements,
    replay_class: ReplayClass,
}

impl PreparedToolCall {
    fn from_parts(
        intent: &ToolIntent,
        definition: &ToolDefinition,
        arguments: Value,
    ) -> Result<Self, PreparationError> {
        let normalized_arguments = serde_jcs::to_string(&arguments)
            .map_err(|_| PreparationError::new(PreparationErrorCode::NonCanonicalValue))?;
        let preimage = json!({ "contract": "garive.prepared-tool-call", "version": 1, "tool_name": definition.name, "tool_revision": definition.revision, "arguments": arguments, "requirements": definition.requirements, "replay_class": definition.replay_class });
        let canonical = serde_jcs::to_vec(&preimage)
            .map_err(|_| PreparationError::new(PreparationErrorCode::NonCanonicalValue))?;
        let input_digest = format!("{:x}", Sha256::digest(canonical));
        Ok(Self {
            model_call_id: intent.model_call_id.clone(),
            tool_name: definition.name.clone(),
            tool_revision: definition.revision.clone(),
            normalized_arguments,
            input_digest,
            requirements: definition.requirements.clone(),
            replay_class: definition.replay_class,
        })
    }
    /// Returns untrusted model correlation retained for observations.
    pub fn model_call_id(&self) -> &str {
        &self.model_call_id
    }
    /// Returns the exact admitted tool name.
    pub fn tool_name(&self) -> &str {
        &self.tool_name
    }
    /// Returns the exact admitted definition revision.
    pub fn tool_revision(&self) -> &str {
        &self.tool_revision
    }
    /// Returns RFC 8785 canonical argument JSON.
    pub fn normalized_arguments(&self) -> &str {
        &self.normalized_arguments
    }
    /// Returns the lowercase SHA-256 executable-input digest.
    pub fn input_digest(&self) -> &str {
        &self.input_digest
    }
    /// Returns immutable execution requirements.
    pub const fn requirements(&self) -> &ExecutionRequirements {
        &self.requirements
    }
    /// Returns the untrusted recovery declaration.
    pub const fn replay_class(&self) -> ReplayClass {
        self.replay_class
    }
}

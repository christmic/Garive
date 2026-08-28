use std::collections::BTreeSet;

use crate::MediaKind;

#[derive(Clone, Debug, Eq, PartialEq)]
/// Opaque identity for one logical provider-neutral model request.
pub struct ModelRequestId(String);

impl ModelRequestId {
    /// Wraps an identity; [`ModelRequest::validate`] rejects an empty value.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the opaque identity as supplied by Runtime.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Opaque identity of a frozen model capability target.
pub struct ModelTargetId(String);

impl ModelTargetId {
    /// Wraps a target identity; [`ModelRequest::validate`] rejects an empty value.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the opaque target identity.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
/// Provider-neutral capability required from the selected target.
pub enum ModelCapability {
    /// Text input and output.
    Text,
    /// Image/media understanding.
    Vision,
    /// Reasoning items or references.
    Reasoning,
    /// Tool definition admission and tool intent output.
    Tools,
    /// Structured JSON output constraints.
    JsonOutput,
    /// Normalized live stream events.
    Streaming,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Semantic role of a message in ordered model input.
pub enum ModelRole {
    /// Highest-authority system instruction.
    System,
    /// Application/developer instruction.
    Developer,
    /// User-authored content.
    User,
    /// Prior model-authored content.
    Assistant,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// One content part inside a provider-neutral input message.
pub enum ModelInputContent {
    /// UTF-8 text content.
    Text(String),
    /// External media reference with an asserted media type.
    MediaReference {
        /// Provider-neutral media class.
        media_kind: MediaKind,
        /// Runtime/adaptor-resolvable content reference.
        reference: String,
        /// Declared MIME/media type.
        media_type: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Ordered input item admitted to a model request.
pub enum ModelInputItem {
    /// Role-bearing message with one or more content parts.
    Message {
        /// Semantic message role.
        role: ModelRole,
        /// Ordered message content parts.
        content: Vec<ModelInputContent>,
    },
    /// Neutral tool result correlated to a prior model call.
    ToolObservation {
        /// Model-owned call correlation identity.
        model_call_id: String,
        /// Structured neutral result encoded as JSON text.
        result_json: String,
    },
    /// Opaque reasoning state returned to a compatible provider.
    ReasoningReference {
        /// Provider-issued opaque reference.
        reference: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Frozen tool definition exposed to a model for one request.
pub struct ToolDescriptor {
    /// Stable tool name used by model intents.
    pub name: String,
    /// Human-readable behavior description.
    pub description: String,
    /// Exact definition revision bound to authorization and replay.
    pub definition_revision: String,
    /// JSON Schema text for structured input arguments.
    pub input_schema_json: String,
    /// Whether adapters should request strict schema enforcement when supported.
    pub strict: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Requested response text/structure mode.
pub enum TextMode {
    /// Unconstrained plain text.
    Plain,
    /// A syntactically valid JSON object without a supplied schema.
    JsonObject,
    /// JSON output constrained by an exact schema.
    JsonSchema {
        /// JSON Schema text supplied to compatible adapters.
        schema_json: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Provider-neutral output constraints for one request.
pub struct ModelOutputSettings {
    /// Optional non-zero generated-token limit.
    pub max_output_tokens: Option<u64>,
    /// Requested plain or structured response mode.
    pub text_mode: TextMode,
    /// Whether model-visible reasoning content is admitted in normalized output.
    pub reasoning_visibility: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Immutable provider-neutral request accepted by [`crate::ModelPort`].
pub struct ModelRequest {
    /// Logical request identity used across lifecycle facts.
    pub request_id: ModelRequestId,
    /// Frozen target selected by Runtime/Core policy.
    pub target_id: ModelTargetId,
    /// Deduplicated capabilities the adapter must support.
    pub required_capabilities: Vec<ModelCapability>,
    /// Ordered context/input items.
    pub input_items: Vec<ModelInputItem>,
    /// Deduplicated frozen tool definitions.
    pub tools: Vec<ToolDescriptor>,
    /// Provider-neutral response constraints.
    pub output: ModelOutputSettings,
    /// Bounded, secret-free trace metadata forwarded by adapters.
    pub trace_metadata: Vec<(String, String)>,
}

impl ModelRequest {
    /// Validates identities, duplicates, tool definitions, limits, and metadata.
    pub fn validate(&self) -> Result<(), RequestValidationError> {
        if self.request_id.0.is_empty() || self.target_id.0.is_empty() {
            return Err(RequestValidationError::EmptyIdentity);
        }
        let capabilities: BTreeSet<_> = self.required_capabilities.iter().collect();
        if capabilities.len() != self.required_capabilities.len() {
            return Err(RequestValidationError::DuplicateCapability);
        }
        let mut tools = BTreeSet::new();
        for tool in &self.tools {
            if tool.name.is_empty()
                || tool.definition_revision.is_empty()
                || tool.input_schema_json.is_empty()
            {
                return Err(RequestValidationError::InvalidTool);
            }
            if !tools.insert(tool.name.as_str()) {
                return Err(RequestValidationError::DuplicateTool);
            }
        }
        if self.output.max_output_tokens == Some(0) {
            return Err(RequestValidationError::ZeroOutputLimit);
        }
        let mut metadata = BTreeSet::new();
        for (key, value) in &self.trace_metadata {
            if key.is_empty() || key.len() > 64 || value.len() > 512 {
                return Err(RequestValidationError::InvalidMetadata);
            }
            if !metadata.insert(key.as_str()) {
                return Err(RequestValidationError::DuplicateMetadata);
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Stable validation failure for a provider-neutral model request.
pub enum RequestValidationError {
    /// Request or target identity is empty.
    EmptyIdentity,
    /// A required capability appears more than once.
    DuplicateCapability,
    /// Tool name, revision, or schema is empty.
    InvalidTool,
    /// Two admitted tools use the same name.
    DuplicateTool,
    /// Optional maximum output tokens was explicitly set to zero.
    ZeroOutputLimit,
    /// Metadata key is empty/too long or its value is too long.
    InvalidMetadata,
    /// Two metadata entries use the same key.
    DuplicateMetadata,
}

impl RequestValidationError {
    /// Returns the stable machine-readable validation code.
    pub const fn code(self) -> &'static str {
        match self {
            Self::EmptyIdentity => "empty-identity",
            Self::DuplicateCapability => "duplicate-capability",
            Self::InvalidTool => "invalid-tool",
            Self::DuplicateTool => "duplicate-tool",
            Self::ZeroOutputLimit => "zero-output-limit",
            Self::InvalidMetadata => "invalid-metadata",
            Self::DuplicateMetadata => "duplicate-metadata",
        }
    }
}

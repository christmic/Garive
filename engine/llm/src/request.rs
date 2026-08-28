use std::collections::BTreeSet;

use crate::MediaKind;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelRequestId(String);

impl ModelRequestId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelTargetId(String);

impl ModelTargetId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ModelCapability {
    Text,
    Vision,
    Reasoning,
    Tools,
    JsonOutput,
    Streaming,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModelRole {
    System,
    Developer,
    User,
    Assistant,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ModelInputContent {
    Text(String),
    MediaReference {
        media_kind: MediaKind,
        reference: String,
        media_type: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ModelInputItem {
    Message {
        role: ModelRole,
        content: Vec<ModelInputContent>,
    },
    ToolObservation {
        model_call_id: String,
        result_json: String,
    },
    ReasoningReference {
        reference: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolDescriptor {
    pub name: String,
    pub description: String,
    pub definition_revision: String,
    pub input_schema_json: String,
    pub strict: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TextMode {
    Plain,
    JsonObject,
    JsonSchema { schema_json: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelOutputSettings {
    pub max_output_tokens: Option<u64>,
    pub text_mode: TextMode,
    pub reasoning_visibility: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelRequest {
    pub request_id: ModelRequestId,
    pub target_id: ModelTargetId,
    pub required_capabilities: Vec<ModelCapability>,
    pub input_items: Vec<ModelInputItem>,
    pub tools: Vec<ToolDescriptor>,
    pub output: ModelOutputSettings,
    pub trace_metadata: Vec<(String, String)>,
}

impl ModelRequest {
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
pub enum RequestValidationError {
    EmptyIdentity,
    DuplicateCapability,
    InvalidTool,
    DuplicateTool,
    ZeroOutputLimit,
    InvalidMetadata,
    DuplicateMetadata,
}

impl RequestValidationError {
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

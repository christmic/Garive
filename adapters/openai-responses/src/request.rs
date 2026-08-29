//! Typed portable create-request profile derived from the official SDK.

use crate::{HttpRequest, ResponsesAdapter, ResponsesAdapterError};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

/// A complete create request for the portable Responses profile.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CreateResponseRequest {
    /// Deployment-selected model identifier.
    pub model: String,
    /// String shorthand or ordered input items.
    pub input: ResponseInput,
    /// Maximum generated tokens when bounded by the caller.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u64>,
    /// Sampling temperature.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    /// Nucleus sampling probability.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    /// Context truncation behavior.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncation: Option<Truncation>,
    /// Client-executed function tools.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ResponseTool>,
    /// Tool selection policy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    /// Whether independent function calls may be emitted together.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
    /// Text and structured-output settings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<ResponseTextConfig>,
    /// Reasoning effort and summary settings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<ReasoningConfig>,
    /// Caller metadata preserved as protocol strings.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, String>,
    /// Whether the exchange returns SSE.
    pub stream: bool,
    /// Optional portable streaming controls.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<StreamOptions>,
    /// Provider-admitted non-colliding protocol fields.
    #[serde(default, flatten)]
    pub extensions: Map<String, Value>,
}

impl CreateResponseRequest {
    /// Creates a request with no implicit optional behavior.
    pub fn new(model: impl Into<String>, input: ResponseInput, stream: bool) -> Self {
        Self {
            model: model.into(),
            input,
            max_output_tokens: None,
            temperature: None,
            top_p: None,
            truncation: None,
            tools: Vec::new(),
            tool_choice: None,
            parallel_tool_calls: None,
            text: None,
            reasoning: None,
            metadata: BTreeMap::new(),
            stream,
            stream_options: None,
            extensions: Map::new(),
        }
    }

    /// Validates portable-profile invariants before serialization.
    pub fn validate(&self) -> Result<(), ResponsesAdapterError> {
        require_text(&self.model, "Responses model must not be empty")?;
        self.input.validate()?;
        finite_range(self.temperature, 0.0, 2.0, "invalid Responses temperature")?;
        finite_range(self.top_p, 0.0, 1.0, "invalid Responses top_p")?;
        if self.metadata.len() > 16
            || self
                .metadata
                .iter()
                .any(|(key, value)| key.is_empty() || key.len() > 64 || value.len() > 512)
        {
            return Err(ResponsesAdapterError::InvalidRequest(
                "Responses metadata exceeds the portable bounds",
            ));
        }
        let mut tool_names = BTreeSet::new();
        for tool in &self.tools {
            let ResponseTool::Function(function) = tool;
            function.validate()?;
            if !tool_names.insert(function.name.as_str()) {
                return Err(ResponsesAdapterError::InvalidRequest(
                    "Responses function tool names must be unique",
                ));
            }
        }
        if let Some(ToolChoice::Function { name, .. }) = &self.tool_choice {
            require_text(name, "Responses selected function must not be empty")?;
        }
        if let Some(text) = &self.text {
            text.validate()?;
        }
        reject_collisions(&self.extensions)?;
        Ok(())
    }
}

impl ResponsesAdapter {
    /// Validates and encodes one typed create request.
    pub fn prepare(
        &self,
        request: &CreateResponseRequest,
    ) -> Result<HttpRequest, ResponsesAdapterError> {
        request.validate()?;
        let body = serde_json::to_vec(request).map_err(|_| ResponsesAdapterError::InvalidJson)?;
        Ok(self.build_request(body, request.stream))
    }
}

/// Responses input string shorthand or ordered item list.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseInput {
    /// A single user-text shorthand.
    Text(String),
    /// Ordered typed input items.
    Items(Vec<InputItem>),
}

impl ResponseInput {
    fn validate(&self) -> Result<(), ResponsesAdapterError> {
        match self {
            Self::Text(text) => require_text(text, "Responses input text must not be empty"),
            Self::Items(items) if items.is_empty() => Err(ResponsesAdapterError::InvalidRequest(
                "Responses input items must not be empty",
            )),
            Self::Items(items) => items.iter().try_for_each(InputItem::validate),
        }
    }
}

/// Portable input item union.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InputItem {
    /// A role-bearing message.
    Message {
        /// Message role.
        role: MessageRole,
        /// Ordered message content.
        content: Vec<InputContent>,
    },
    /// Result returned for a prior client function call.
    FunctionCallOutput(FunctionCallOutput),
}

impl InputItem {
    fn validate(&self) -> Result<(), ResponsesAdapterError> {
        match self {
            Self::Message { content, .. } if content.is_empty() => {
                Err(ResponsesAdapterError::InvalidRequest(
                    "Responses message content must not be empty",
                ))
            }
            Self::Message { content, .. } => content.iter().try_for_each(InputContent::validate),
            Self::FunctionCallOutput(output) => output.validate(),
        }
    }
}

/// Roles admitted by portable Responses message input.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    /// System policy text.
    System,
    /// Developer instruction text.
    Developer,
    /// User input.
    User,
    /// Prior assistant output.
    Assistant,
}

/// Portable input content block.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InputContent {
    /// Text supplied to the model.
    InputText {
        /// Text value.
        text: String,
    },
    /// Image supplied by URL or opaque file identifier.
    InputImage {
        /// Optional URL or data URL.
        #[serde(skip_serializing_if = "Option::is_none")]
        image_url: Option<String>,
        /// Optional deployment-resolved file identifier.
        #[serde(skip_serializing_if = "Option::is_none")]
        file_id: Option<String>,
        /// Optional detail hint.
        #[serde(skip_serializing_if = "Option::is_none")]
        detail: Option<ImageDetail>,
    },
}

impl InputContent {
    fn validate(&self) -> Result<(), ResponsesAdapterError> {
        match self {
            Self::InputText { text } => {
                require_text(text, "Responses input text must not be empty")
            }
            Self::InputImage {
                image_url, file_id, ..
            } if image_url.is_some() == file_id.is_some() => {
                Err(ResponsesAdapterError::InvalidRequest(
                    "Responses image requires exactly one reference",
                ))
            }
            Self::InputImage {
                image_url, file_id, ..
            } => require_text(
                image_url
                    .as_ref()
                    .or(file_id.as_ref())
                    .expect("validated branch"),
                "Responses image reference must not be empty",
            ),
        }
    }
}

/// Image fidelity hint from the official create shape.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageDetail {
    /// Let the implementation select fidelity.
    Auto,
    /// Request lower detail.
    Low,
    /// Request higher detail.
    High,
}

/// Portable function call result item.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FunctionCallOutput {
    /// Correlation identifier emitted by the model.
    pub call_id: String,
    /// String or official ordered result-content value.
    pub output: FunctionOutput,
    /// Optional lifecycle status.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<ItemStatus>,
}

impl FunctionCallOutput {
    fn validate(&self) -> Result<(), ResponsesAdapterError> {
        require_text(&self.call_id, "Responses call_id must not be empty")?;
        self.output.validate()
    }
}

/// Official function-result string or ordered portable content union.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FunctionOutput {
    /// String shorthand result.
    Text(String),
    /// Ordered text/image result content.
    Content(Vec<InputContent>),
}

impl FunctionOutput {
    fn validate(&self) -> Result<(), ResponsesAdapterError> {
        match self {
            Self::Text(_) => Ok(()),
            Self::Content(content) if content.is_empty() => {
                Err(ResponsesAdapterError::InvalidRequest(
                    "Responses function content must not be empty",
                ))
            }
            Self::Content(content) => content.iter().try_for_each(InputContent::validate),
        }
    }
}

/// Item lifecycle status used by input and output items.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemStatus {
    /// Item generation has started.
    InProgress,
    /// Item generation completed.
    Completed,
    /// Item generation stopped before completion.
    Incomplete,
}

/// Portable tool union.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseTool {
    /// A client-executed JSON function.
    Function(FunctionTool),
}

/// Client function definition.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FunctionTool {
    /// Function name.
    pub name: String,
    /// Optional model-visible description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// JSON Schema object for arguments.
    pub parameters: Map<String, Value>,
    /// Whether schema adherence is strict.
    pub strict: bool,
}

impl FunctionTool {
    fn validate(&self) -> Result<(), ResponsesAdapterError> {
        require_text(&self.name, "Responses function name must not be empty")?;
        Ok(())
    }
}

/// Portable tool-selection union.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolChoice {
    /// A protocol option string: `none`, `auto`, or `required`.
    Mode(ToolChoiceMode),
    /// Select one named client function.
    Function {
        /// Fixed protocol discriminator.
        r#type: FunctionChoiceType,
        /// Selected function name.
        name: String,
    },
}

/// Portable tool choice mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolChoiceMode {
    /// Do not call a tool.
    None,
    /// Let the model decide.
    Auto,
    /// Require at least one tool call.
    Required,
}

/// Fixed `function` discriminator for a named tool choice.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FunctionChoiceType {
    /// Client function selection.
    Function,
}

/// Text response configuration.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResponseTextConfig {
    /// Selected output format.
    pub format: TextFormat,
}

impl ResponseTextConfig {
    fn validate(&self) -> Result<(), ResponsesAdapterError> {
        if let TextFormat::JsonSchema {
            name,
            schema,
            description: _,
            strict: _,
        } = &self.format
        {
            require_text(name, "Responses JSON Schema name must not be empty")?;
            let _ = schema;
        }
        Ok(())
    }
}

/// Portable text output formats.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TextFormat {
    /// Ordinary text output.
    Text,
    /// Unnamed JSON object mode.
    JsonObject,
    /// Named JSON Schema output.
    JsonSchema {
        /// Schema name.
        name: String,
        /// Optional model-visible description.
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        /// JSON Schema object.
        schema: Map<String, Value>,
        /// Strict schema adherence.
        strict: bool,
    },
}

/// Optional reasoning controls from the create shape.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReasoningConfig {
    /// Optional effort discriminator retained as a protocol string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<ReasoningEffort>,
    /// Optional summary policy retained as a protocol string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<ReasoningSummary>,
}

/// Official portable reasoning effort values.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningEffort {
    /// Disable reasoning effort.
    None,
    /// Minimal reasoning effort.
    Minimal,
    /// Low reasoning effort.
    Low,
    /// Medium reasoning effort.
    Medium,
    /// High reasoning effort.
    High,
    /// Extra-high reasoning effort.
    Xhigh,
    /// Maximum reasoning effort.
    Max,
}

/// Official reasoning summary modes.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningSummary {
    /// Let the protocol select summary detail.
    Auto,
    /// Concise summary.
    Concise,
    /// Detailed summary.
    Detailed,
}

/// Context truncation selection.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Truncation {
    /// Reject input that exceeds the context window.
    Disabled,
    /// Permit protocol-defined automatic truncation.
    Auto,
}

/// Optional core stream controls.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StreamOptions {
    /// Whether payload padding is requested.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_obfuscation: Option<bool>,
}

fn require_text(value: &str, reason: &'static str) -> Result<(), ResponsesAdapterError> {
    if value.is_empty() {
        Err(ResponsesAdapterError::InvalidRequest(reason))
    } else {
        Ok(())
    }
}

fn finite_range(
    value: Option<f64>,
    minimum: f64,
    maximum: f64,
    reason: &'static str,
) -> Result<(), ResponsesAdapterError> {
    if let Some(value) = value {
        if !value.is_finite() || value < minimum || value > maximum {
            return Err(ResponsesAdapterError::InvalidRequest(reason));
        }
    }
    Ok(())
}

fn reject_collisions(extensions: &Map<String, Value>) -> Result<(), ResponsesAdapterError> {
    const TYPED: &[&str] = &[
        "model",
        "input",
        "max_output_tokens",
        "temperature",
        "top_p",
        "truncation",
        "tools",
        "tool_choice",
        "parallel_tool_calls",
        "text",
        "reasoning",
        "metadata",
        "stream",
        "stream_options",
    ];
    if extensions.keys().any(|key| TYPED.contains(&key.as_str())) {
        Err(ResponsesAdapterError::InvalidRequest(
            "Responses extension collides with a typed field",
        ))
    } else {
        Ok(())
    }
}

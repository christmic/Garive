//! Typed portable create-request profile derived from the official SDK.

use crate::{HttpRequest, MessagesAdapter, MessagesAdapterError};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeSet;

/// A complete create request for the portable Messages profile.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CreateMessageRequest {
    /// Deployment-selected model identifier.
    pub model: String,
    /// Maximum generated tokens; zero is valid for cache pre-warming.
    pub max_tokens: u64,
    /// Ordered user and assistant turns.
    pub messages: Vec<Message>,
    /// Whether the exchange returns SSE.
    pub stream: bool,
    /// Optional top-level system prompt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<SystemPrompt>,
    /// Custom generation stop sequences.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stop_sequences: Vec<String>,
    /// Sampling temperature.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    /// Nucleus sampling probability.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    /// Top-k sampling bound.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u64>,
    /// Client-executed tools.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<Tool>,
    /// Tool selection policy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    /// Structured output and effort configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_config: Option<OutputConfig>,
    /// Extended-thinking configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<ThinkingConfig>,
    /// Protocol metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
    /// Provider-admitted non-colliding protocol fields.
    #[serde(default, flatten)]
    pub extensions: Map<String, Value>,
}

impl CreateMessageRequest {
    /// Creates a request with no implicit optional behavior.
    pub fn new(
        model: impl Into<String>,
        max_tokens: u64,
        messages: Vec<Message>,
        stream: bool,
    ) -> Self {
        Self {
            model: model.into(),
            max_tokens,
            messages,
            stream,
            system: None,
            stop_sequences: Vec::new(),
            temperature: None,
            top_p: None,
            top_k: None,
            tools: Vec::new(),
            tool_choice: None,
            output_config: None,
            thinking: None,
            metadata: None,
            extensions: Map::new(),
        }
    }

    /// Validates portable-profile invariants before serialization.
    pub fn validate(&self) -> Result<(), MessagesAdapterError> {
        require_text(&self.model, "Messages model must not be empty")?;
        if self.messages.is_empty() {
            return Err(MessagesAdapterError::InvalidRequest(
                "Messages turns must not be empty",
            ));
        }
        self.messages.iter().try_for_each(Message::validate)?;
        if let Some(system) = &self.system {
            match system {
                SystemPrompt::Text(text) => {
                    require_text(text, "Messages system text must not be empty")?
                }
                SystemPrompt::Blocks(blocks) if blocks.is_empty() => {
                    return Err(MessagesAdapterError::InvalidRequest(
                        "Messages system blocks must not be empty",
                    ));
                }
                SystemPrompt::Blocks(blocks) => blocks.iter().try_for_each(TextBlock::validate)?,
            }
        }
        finite_range(self.temperature, 0.0, 1.0, "invalid Messages temperature")?;
        finite_range(self.top_p, 0.0, 1.0, "invalid Messages top_p")?;
        if self.stop_sequences.iter().any(String::is_empty) {
            return Err(MessagesAdapterError::InvalidRequest(
                "Messages stop sequence must not be empty",
            ));
        }
        let mut names = BTreeSet::new();
        for tool in &self.tools {
            tool.validate()?;
            if !names.insert(tool.name.as_str()) {
                return Err(MessagesAdapterError::InvalidRequest(
                    "Messages tool names must be unique",
                ));
            }
        }
        if let Some(ToolChoice::Tool { name, .. }) = &self.tool_choice {
            require_text(name, "Messages selected tool must not be empty")?;
        }
        reject_collisions(&self.extensions)
    }
}

impl MessagesAdapter {
    /// Validates and encodes one typed create request.
    pub fn prepare(
        &self,
        request: &CreateMessageRequest,
    ) -> Result<HttpRequest, MessagesAdapterError> {
        request.validate()?;
        let body = serde_json::to_vec(request).map_err(|_| MessagesAdapterError::InvalidJson)?;
        Ok(self.build_request(body, request.stream))
    }
}

/// One conversational turn.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Message {
    /// Turn role.
    pub role: MessageRole,
    /// String shorthand or ordered content blocks.
    pub content: MessageContent,
}

impl Message {
    /// Creates one role-bearing turn.
    pub fn new(role: MessageRole, content: MessageContent) -> Self {
        Self { role, content }
    }

    fn validate(&self) -> Result<(), MessagesAdapterError> {
        match &self.content {
            MessageContent::Text(text) => {
                require_text(text, "Messages content text must not be empty")
            }
            MessageContent::Blocks(blocks) if blocks.is_empty() => Err(
                MessagesAdapterError::InvalidRequest("Messages content blocks must not be empty"),
            ),
            MessageContent::Blocks(blocks) => blocks.iter().try_for_each(ContentBlock::validate),
        }
    }
}

/// Roles admitted by the official Messages request.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    /// User turn.
    User,
    /// Prior assistant turn.
    Assistant,
}

/// String shorthand or ordered request blocks.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    /// Single text shorthand.
    Text(String),
    /// Ordered content blocks.
    Blocks(Vec<ContentBlock>),
}

/// Portable request content block.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    /// Text input.
    Text {
        /// Text value.
        text: String,
        /// Optional prompt-cache marker.
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    /// Image input from an official source union.
    Image {
        /// Image source.
        source: ImageSource,
        /// Optional prompt-cache marker.
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    /// Document input from an official source union.
    Document {
        /// Document source.
        source: DocumentSource,
        /// Optional display title.
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        /// Optional context.
        #[serde(skip_serializing_if = "Option::is_none")]
        context: Option<String>,
    },
    /// Prior client tool invocation.
    ToolUse {
        /// Invocation identifier.
        id: String,
        /// Tool name.
        name: String,
        /// JSON tool input.
        input: Map<String, Value>,
    },
    /// Result for a prior client tool invocation.
    ToolResult {
        /// Invocation identifier.
        tool_use_id: String,
        /// String shorthand or portable result blocks.
        #[serde(skip_serializing_if = "Option::is_none")]
        content: Option<ToolResultContent>,
        /// Whether execution failed.
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
    /// Prior extended-thinking block.
    Thinking {
        /// Thinking text.
        thinking: String,
        /// Integrity signature.
        signature: String,
    },
    /// Opaque prior redacted thinking.
    RedactedThinking {
        /// Opaque protocol data.
        data: String,
    },
}

impl ContentBlock {
    fn validate(&self) -> Result<(), MessagesAdapterError> {
        match self {
            Self::Text { text, .. } => require_text(text, "Messages text block must not be empty"),
            Self::ToolUse { id, name, .. } => {
                require_text(id, "Messages tool-use id must not be empty")?;
                require_text(name, "Messages tool-use name must not be empty")
            }
            Self::ToolResult { tool_use_id, .. } => {
                require_text(tool_use_id, "Messages tool-result id must not be empty")
            }
            Self::Thinking {
                thinking,
                signature,
            } => {
                require_text(thinking, "Messages thinking must not be empty")?;
                require_text(signature, "Messages thinking signature must not be empty")
            }
            Self::RedactedThinking { data } => {
                require_text(data, "Messages redacted thinking data must not be empty")
            }
            Self::Image { .. } | Self::Document { .. } => Ok(()),
        }
    }
}

/// Text block with optional cache-control wire data.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextBlock {
    /// Fixed protocol discriminator required by the official shape.
    #[serde(rename = "type")]
    pub kind: TextBlockType,
    /// Text value.
    pub text: String,
    /// Optional prompt-cache marker.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

/// Text-block discriminator.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextBlockType {
    /// Text content.
    Text,
}

impl TextBlock {
    fn validate(&self) -> Result<(), MessagesAdapterError> {
        require_text(&self.text, "Messages text block must not be empty")
    }
}

/// Official ephemeral cache marker carried as wire data.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CacheControl {
    /// Fixed protocol discriminator.
    #[serde(rename = "type")]
    pub kind: CacheControlType,
    /// Optional protocol time-to-live.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl: Option<String>,
}

/// Cache-control discriminator.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheControlType {
    /// Ephemeral prompt cache entry.
    Ephemeral,
}

/// Image source union.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ImageSource {
    /// Base64 encoded image.
    Base64 {
        /// Media type.
        media_type: String,
        /// Encoded bytes.
        data: String,
    },
    /// URL-referenced image.
    Url {
        /// Absolute or caller-admitted URL.
        url: String,
    },
}

/// Document source union.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DocumentSource {
    /// Base64 encoded PDF.
    Base64 {
        /// Fixed PDF media type.
        media_type: String,
        /// Encoded bytes.
        data: String,
    },
    /// URL-referenced PDF.
    Url {
        /// PDF URL.
        url: String,
    },
    /// Plain text document.
    Text {
        /// Text data.
        data: String,
        /// Optional media type.
        #[serde(skip_serializing_if = "Option::is_none")]
        media_type: Option<String>,
    },
    /// Nested content blocks.
    Content {
        /// Portable nested content.
        content: Vec<TextBlock>,
    },
}

/// Tool result string shorthand or portable blocks.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolResultContent {
    /// String shorthand.
    Text(String),
    /// Portable result blocks retained losslessly.
    Blocks(Vec<Value>),
}

/// Top-level system prompt.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SystemPrompt {
    /// String shorthand.
    Text(String),
    /// Ordered text blocks.
    Blocks(Vec<TextBlock>),
}

/// Client-executed tool definition.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tool {
    /// Tool name.
    pub name: String,
    /// JSON Schema input object.
    pub input_schema: Map<String, Value>,
    /// Optional tool description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Optional strict schema enforcement.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    /// Optional prompt-cache marker carried as protocol data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_control: Option<CacheControl>,
}

impl Tool {
    fn validate(&self) -> Result<(), MessagesAdapterError> {
        require_text(&self.name, "Messages tool name must not be empty")
    }
}

/// Tool selection policy.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolChoice {
    /// Model-selected tool use.
    Auto {
        /// Whether parallel calls are disabled.
        #[serde(skip_serializing_if = "Option::is_none")]
        disable_parallel_tool_use: Option<bool>,
    },
    /// Any available tool.
    Any {
        /// Whether parallel calls are disabled.
        #[serde(skip_serializing_if = "Option::is_none")]
        disable_parallel_tool_use: Option<bool>,
    },
    /// One named tool.
    Tool {
        /// Tool name.
        name: String,
        /// Whether parallel calls are disabled.
        #[serde(skip_serializing_if = "Option::is_none")]
        disable_parallel_tool_use: Option<bool>,
    },
    /// No tool use.
    None {
        /// Whether parallel calls are disabled.
        #[serde(skip_serializing_if = "Option::is_none")]
        disable_parallel_tool_use: Option<bool>,
    },
}

/// Structured output and effort configuration.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct OutputConfig {
    /// Optional effort literal retained as protocol text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    /// Official JSON output format object.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<Map<String, Value>>,
}

/// Extended-thinking configuration.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ThinkingConfig {
    /// Disabled thinking.
    Disabled,
    /// Enabled with a token budget.
    Enabled {
        /// Thinking token budget.
        budget_tokens: u64,
    },
    /// Adaptive thinking.
    Adaptive,
}

/// Protocol request metadata.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Metadata {
    /// Opaque external user identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
}

fn require_text(value: &str, reason: &'static str) -> Result<(), MessagesAdapterError> {
    if value.is_empty() {
        Err(MessagesAdapterError::InvalidRequest(reason))
    } else {
        Ok(())
    }
}

fn finite_range(
    value: Option<f64>,
    min: f64,
    max: f64,
    reason: &'static str,
) -> Result<(), MessagesAdapterError> {
    if value.is_some_and(|value| !value.is_finite() || value < min || value > max) {
        Err(MessagesAdapterError::InvalidRequest(reason))
    } else {
        Ok(())
    }
}

fn reject_collisions(extensions: &Map<String, Value>) -> Result<(), MessagesAdapterError> {
    const RESERVED: [&str; 15] = [
        "model",
        "max_tokens",
        "messages",
        "stream",
        "system",
        "stop_sequences",
        "temperature",
        "top_p",
        "top_k",
        "tools",
        "tool_choice",
        "output_config",
        "thinking",
        "metadata",
        "extensions",
    ];
    if extensions
        .keys()
        .any(|key| RESERVED.contains(&key.as_str()))
    {
        Err(MessagesAdapterError::InvalidRequest(
            "Messages extension collides with a typed field",
        ))
    } else {
        Ok(())
    }
}

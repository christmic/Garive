//! Typed portable create-request profile derived from the official SDK.

use crate::{wire, HttpRequest, MessagesAdapter, MessagesAdapterError};
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
        if let Some(ThinkingConfig::Enabled { budget_tokens, .. }) = &self.thinking {
            if *budget_tokens < 1_024 || *budget_tokens >= self.max_tokens {
                return Err(MessagesAdapterError::InvalidRequest(
                    "Messages thinking budget must be at least 1024 and below max_tokens",
                ));
            }
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
        /// Optional prompt-cache marker.
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
        /// Optional citation generation setting.
        #[serde(skip_serializing_if = "Option::is_none")]
        citations: Option<CitationsConfig>,
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
        /// Optional prompt-cache marker.
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
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
        /// Optional prompt-cache marker.
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
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
            Self::Image { source, .. } => source.validate(),
            Self::Document { source, .. } => source.validate(),
            Self::ToolUse { id, name, .. } => {
                require_text(id, "Messages tool-use id must not be empty")?;
                require_text(name, "Messages tool-use name must not be empty")
            }
            Self::ToolResult {
                tool_use_id,
                content,
                ..
            } => {
                require_text(tool_use_id, "Messages tool-result id must not be empty")?;
                if let Some(content) = content {
                    content.validate()?;
                }
                Ok(())
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
    /// Optional citation objects retained losslessly.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citations: Option<Vec<Value>>,
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
    pub ttl: Option<CacheTtl>,
}

/// Cache-control discriminator.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheControlType {
    /// Ephemeral prompt cache entry.
    Ephemeral,
}

/// Official prompt-cache time-to-live values.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum CacheTtl {
    /// Five-minute cache entry.
    #[serde(rename = "5m")]
    FiveMinutes,
    /// One-hour cache entry.
    #[serde(rename = "1h")]
    OneHour,
}

/// Image source union.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ImageSource {
    /// Base64 encoded image.
    Base64 {
        /// Media type.
        media_type: ImageMediaType,
        /// Encoded bytes.
        data: String,
    },
    /// URL-referenced image.
    Url {
        /// Absolute or caller-admitted URL.
        url: String,
    },
}

impl ImageSource {
    fn validate(&self) -> Result<(), MessagesAdapterError> {
        match self {
            Self::Base64 { data, .. } => {
                require_text(data, "Messages base64 image data must not be empty")
            }
            Self::Url { url } => require_text(url, "Messages image URL must not be empty"),
        }
    }
}

/// Official base64 image media types.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ImageMediaType {
    /// JPEG image.
    #[serde(rename = "image/jpeg")]
    Jpeg,
    /// PNG image.
    #[serde(rename = "image/png")]
    Png,
    /// GIF image.
    #[serde(rename = "image/gif")]
    Gif,
    /// WebP image.
    #[serde(rename = "image/webp")]
    Webp,
}

/// Document source union.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DocumentSource {
    /// Base64 encoded PDF.
    Base64 {
        /// Fixed PDF media type.
        media_type: PdfMediaType,
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
        /// Fixed plain-text media type.
        media_type: TextMediaType,
    },
    /// Nested content blocks.
    Content {
        /// Portable nested content.
        content: DocumentContent,
    },
}

impl DocumentSource {
    fn validate(&self) -> Result<(), MessagesAdapterError> {
        match self {
            Self::Base64 { data, .. } => {
                require_text(data, "Messages base64 PDF data must not be empty")
            }
            Self::Url { url } => require_text(url, "Messages document URL must not be empty"),
            Self::Text { data, .. } => {
                require_text(data, "Messages document text must not be empty")
            }
            Self::Content { content } => content.validate(),
        }
    }
}

/// Fixed base64 PDF media type.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum PdfMediaType {
    /// PDF document.
    #[serde(rename = "application/pdf")]
    Pdf,
}

/// Fixed plain-text document media type.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TextMediaType {
    /// Plain UTF-8 text.
    #[serde(rename = "text/plain")]
    Plain,
}

/// Content-source string shorthand or text/image blocks.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DocumentContent {
    /// String shorthand.
    Text(String),
    /// Ordered text/image source blocks.
    Blocks(Vec<DocumentContentBlock>),
}

impl DocumentContent {
    fn validate(&self) -> Result<(), MessagesAdapterError> {
        match self {
            Self::Text(text) => require_text(text, "Messages document content must not be empty"),
            Self::Blocks(blocks) if blocks.is_empty() => Err(MessagesAdapterError::InvalidRequest(
                "Messages document content blocks must not be empty",
            )),
            Self::Blocks(blocks) => blocks.iter().try_for_each(DocumentContentBlock::validate),
        }
    }
}

/// Portable nested document-source content block.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DocumentContentBlock {
    /// Nested text.
    Text {
        /// Text value.
        text: String,
        /// Optional prompt-cache marker.
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    /// Nested image.
    Image {
        /// Image source.
        source: ImageSource,
        /// Optional prompt-cache marker.
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
}

impl DocumentContentBlock {
    fn validate(&self) -> Result<(), MessagesAdapterError> {
        match self {
            Self::Text { text, .. } => {
                require_text(text, "Messages nested document text must not be empty")
            }
            Self::Image { source, .. } => source.validate(),
        }
    }
}

/// Citation generation setting for document blocks.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct CitationsConfig {
    /// Whether citations are enabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

/// Tool result string shorthand or portable blocks.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolResultContent {
    /// String shorthand.
    Text(String),
    /// Ordered portable result blocks.
    Blocks(Vec<ToolResultBlock>),
}

impl ToolResultContent {
    fn validate(&self) -> Result<(), MessagesAdapterError> {
        match self {
            Self::Text(_) => Ok(()),
            Self::Blocks(blocks) if blocks.is_empty() => Err(MessagesAdapterError::InvalidRequest(
                "Messages tool-result blocks must not be empty",
            )),
            Self::Blocks(blocks) => blocks.iter().try_for_each(ToolResultBlock::validate),
        }
    }
}

/// Portable tool-result content block.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolResultBlock {
    /// Text tool result.
    Text {
        /// Text value.
        text: String,
        /// Optional prompt-cache marker.
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
        /// Optional citation objects.
        #[serde(skip_serializing_if = "Option::is_none")]
        citations: Option<Vec<Value>>,
    },
    /// Image tool result.
    Image {
        /// Image source.
        source: ImageSource,
        /// Optional prompt-cache marker.
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
    /// Document tool result.
    Document {
        /// Document source.
        source: DocumentSource,
        /// Optional prompt-cache marker.
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
        /// Optional citation generation setting.
        #[serde(skip_serializing_if = "Option::is_none")]
        citations: Option<CitationsConfig>,
        /// Optional display title.
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        /// Optional context.
        #[serde(skip_serializing_if = "Option::is_none")]
        context: Option<String>,
    },
}

impl ToolResultBlock {
    fn validate(&self) -> Result<(), MessagesAdapterError> {
        match self {
            Self::Text { text, .. } => {
                require_text(text, "Messages tool-result text must not be empty")
            }
            Self::Image { source, .. } => source.validate(),
            Self::Document { source, .. } => source.validate(),
        }
    }
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
    None,
}

/// Structured output and effort configuration.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct OutputConfig {
    /// Optional effort literal retained as protocol text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<Effort>,
    /// Official JSON output format object.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<JsonOutputFormat>,
}

/// Official output effort levels.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effort {
    /// Low effort.
    Low,
    /// Medium effort.
    Medium,
    /// High effort.
    High,
    /// Extra-high effort.
    Xhigh,
    /// Maximum effort.
    Max,
}

/// Official JSON Schema output format.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JsonOutputFormat {
    /// Fixed format discriminator.
    #[serde(rename = "type")]
    pub kind: JsonOutputFormatType,
    /// Output JSON Schema object.
    pub schema: Map<String, Value>,
}

/// JSON output format discriminator.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JsonOutputFormatType {
    /// JSON Schema output.
    JsonSchema,
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
        /// Optional display policy.
        #[serde(skip_serializing_if = "Option::is_none")]
        display: Option<ThinkingDisplay>,
    },
    /// Adaptive thinking.
    Adaptive {
        /// Optional display policy.
        #[serde(skip_serializing_if = "Option::is_none")]
        display: Option<ThinkingDisplay>,
    },
}

/// Extended-thinking display policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThinkingDisplay {
    /// Return summarized thinking.
    Summarized,
    /// Omit thinking text while preserving signatures.
    Omitted,
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
    if extensions
        .keys()
        .any(|key| wire::CREATE_FIELDS.contains(&key.as_str()))
    {
        Err(MessagesAdapterError::InvalidRequest(
            "Messages extension collides with a typed field",
        ))
    } else {
        Ok(())
    }
}

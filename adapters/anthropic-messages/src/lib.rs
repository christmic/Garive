//! Provider-independent Anthropic Messages-compatible protocol adapter.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod config;
mod error;
mod request;
mod response;

pub use config::{Header, HttpRequest, MessagesAdapter, MessagesAdapterConfig};
pub use error::MessagesAdapterError;
pub use request::{
    CacheControl, CacheControlType, ContentBlock, CreateMessageRequest, DocumentSource,
    ImageSource, Message, MessageContent, MessageRole, Metadata, OutputConfig, SystemPrompt,
    TextBlock, TextBlockType, ThinkingConfig, Tool, ToolChoice, ToolResultContent,
};
pub use response::{
    ApiError, DecodedResponse, ErrorEnvelope, MessageResponse, OutputBlock, OutputRedactedThinking,
    OutputRedactedThinkingType, OutputText, OutputTextType, OutputThinking, OutputThinkingType,
    OutputToolUse, OutputToolUseType, ResponseRole, ResponseType, StopReason, Usage,
};

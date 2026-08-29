//! Provider-independent Anthropic Messages-compatible protocol adapter.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod config;
mod error;
mod request;

pub use config::{Header, HttpRequest, MessagesAdapter, MessagesAdapterConfig};
pub use error::MessagesAdapterError;
pub use request::{
    CacheControl, CacheControlType, ContentBlock, CreateMessageRequest, DocumentSource,
    ImageSource, Message, MessageContent, MessageRole, Metadata, OutputConfig, SystemPrompt,
    TextBlock, TextBlockType, ThinkingConfig, Tool, ToolChoice, ToolResultContent,
};

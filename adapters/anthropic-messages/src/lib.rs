//! Provider-independent Anthropic Messages-compatible protocol adapter.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod config;
mod error;
mod request;
mod response;
mod sse;
mod stream;

pub use config::{Header, HttpRequest, MessagesAdapter, MessagesAdapterConfig};
pub use error::MessagesAdapterError;
pub use request::{
    CacheControl, CacheControlType, CacheTtl, CitationsConfig, ContentBlock, CreateMessageRequest,
    DocumentContent, DocumentContentBlock, DocumentSource, Effort, ImageMediaType, ImageSource,
    JsonOutputFormat, JsonOutputFormatType, Message, MessageContent, MessageRole, Metadata,
    OutputConfig, PdfMediaType, SystemPrompt, TextBlock, TextBlockType, TextMediaType,
    ThinkingConfig, ThinkingDisplay, Tool, ToolChoice, ToolResultBlock, ToolResultContent,
};
pub use response::{
    ApiError, DecodedResponse, ErrorEnvelope, MessageResponse, OutputBlock, OutputRedactedThinking,
    OutputRedactedThinkingType, OutputText, OutputTextType, OutputThinking, OutputThinkingType,
    OutputToolUse, OutputToolUseType, ResponseRole, ResponseType, StopReason, Usage,
};
pub use sse::{SseDecoder, SseFrame};
pub use stream::{DeltaKind, MessagesStreamDecoder, StreamEvent, StreamEventKind};

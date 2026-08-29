//! Provider-independent OpenAI Responses-compatible protocol adapter.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod config;
mod error;
mod items;
mod request;

pub use config::{Header, HttpRequest, ResponsesAdapter, ResponsesAdapterConfig};
pub use error::ResponsesAdapterError;
pub use items::{
    AssistantRole, OutputContent, OutputRefusal, OutputText, ProtocolExtension, ReasoningPart,
    ResponseFunctionCall, ResponseMessage, ResponseOutputItem, ResponseReasoning,
};
pub use request::{
    CreateResponseRequest, FunctionCallOutput, FunctionChoiceType, FunctionTool, ImageDetail,
    InputContent, InputItem, ItemStatus, MessageRole, ReasoningConfig, ResponseInput,
    ResponseTextConfig, ResponseTool, StreamOptions, TextFormat, ToolChoice, ToolChoiceMode,
    Truncation,
};

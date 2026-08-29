//! Provider composition for portable Responses- and Messages-compatible deployments.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod deployment;
mod error;
mod request;

pub use deployment::{
    ErrorDisposition, ErrorSignature, MessagesDeployment, MessagesMediaBinding,
    ProtocolErrorPolicy, ResponsesDeployment, ResponsesMediaBinding,
};
pub use error::CompatibleProviderError;
pub use request::{map_messages_request, map_responses_request};

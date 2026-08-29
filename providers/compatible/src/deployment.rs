use std::collections::{BTreeMap, BTreeSet};

use garive_anthropic_messages::{DocumentSource, ImageSource, ThinkingConfig};
use garive_llm::{InterruptionKind, ModelCapability, RejectionKind, UnavailableKind};
use garive_openai_responses::{ImageDetail, ReasoningConfig};

/// Immutable configuration for one Responses-compatible deployment.
#[derive(Clone, Debug, PartialEq)]
pub struct ResponsesDeployment {
    /// Exact neutral target admitted by this deployment.
    pub target_id: String,
    /// Protocol model identifier selected by configuration.
    pub model_id: String,
    /// Capabilities explicitly admitted by the deployment.
    pub capabilities: BTreeSet<ModelCapability>,
    /// Optional configured output-token default.
    pub default_max_output_tokens: Option<u64>,
    /// Neutral media references resolved to protocol image inputs.
    pub media_bindings: BTreeMap<String, ResponsesMediaBinding>,
    /// Optional protocol reasoning configuration.
    pub reasoning: Option<ReasoningConfig>,
    /// Exact error classification policy.
    pub error_policy: ProtocolErrorPolicy,
}

/// Explicit binding for one neutral Responses media reference.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResponsesMediaBinding {
    /// Bind to a URL or data URL.
    Url {
        /// URL value.
        value: String,
        /// Optional image fidelity.
        detail: Option<ImageDetail>,
    },
    /// Bind to an already uploaded protocol file.
    FileId {
        /// Protocol file identity.
        value: String,
        /// Optional image fidelity.
        detail: Option<ImageDetail>,
    },
}

/// Immutable configuration for one Messages-compatible deployment.
#[derive(Clone, Debug, PartialEq)]
pub struct MessagesDeployment {
    /// Exact neutral target admitted by this deployment.
    pub target_id: String,
    /// Protocol model identifier selected by configuration.
    pub model_id: String,
    /// Capabilities explicitly admitted by the deployment.
    pub capabilities: BTreeSet<ModelCapability>,
    /// Optional configured output-token default.
    pub default_max_output_tokens: Option<u64>,
    /// Neutral media references resolved to protocol content.
    pub media_bindings: BTreeMap<String, MessagesMediaBinding>,
    /// Optional protocol thinking configuration.
    pub thinking: Option<ThinkingConfig>,
    /// Exact error classification policy.
    pub error_policy: ProtocolErrorPolicy,
}

/// Explicit binding for one neutral Messages media reference.
#[derive(Clone, Debug, PartialEq)]
pub enum MessagesMediaBinding {
    /// Bind to an official image source.
    Image(ImageSource),
    /// Bind to an official document source.
    Document(DocumentSource),
}

/// Exact protocol error identity; message text is deliberately excluded.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ErrorSignature {
    /// HTTP status returned by Runtime transport.
    pub status: u16,
    /// Exact protocol error type.
    pub protocol_type: String,
    /// Exact optional protocol error code.
    pub code: Option<String>,
}

/// Provider-neutral disposition assigned to an exact error signature.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorDisposition {
    /// Reject as authentication or authorization failure.
    Rejected(RejectionKind),
    /// Report temporary unavailability.
    Unavailable(UnavailableKind),
    /// Report an interrupted invocation.
    Interrupted(InterruptionKind),
}

/// Immutable table of admitted protocol error mappings.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProtocolErrorPolicy {
    mappings: BTreeMap<ErrorSignature, ErrorDisposition>,
}

impl ProtocolErrorPolicy {
    /// Constructs a policy from exact signatures; duplicate signatures are overwritten last.
    pub fn new(mappings: impl IntoIterator<Item = (ErrorSignature, ErrorDisposition)>) -> Self {
        Self {
            mappings: mappings.into_iter().collect(),
        }
    }

    /// Looks up an exact signature without inspecting human-readable messages.
    pub fn classify(&self, signature: &ErrorSignature) -> Option<ErrorDisposition> {
        self.mappings.get(signature).copied()
    }
}

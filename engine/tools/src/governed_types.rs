//! Portable C5 authority, interaction, and receipt values.

use std::{error::Error, fmt};

use serde_json::Value;

use crate::ExecutionRequirements;

macro_rules! governed_identity {
    ($name:ident, $label:literal) => {
        #[doc = concat!("Non-empty ", $label, " identity owned by Runtime.")]
        #[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
        pub struct $name(Box<str>);

        impl $name {
            #[doc = concat!("Validates and constructs a ", $label, " identity.")]
            pub fn new(value: impl Into<Box<str>>) -> Result<Self, InvalidGovernanceValue> {
                let value = value.into();
                if value.is_empty() {
                    Err(InvalidGovernanceValue($label))
                } else {
                    Ok(Self(value))
                }
            }

            /// Returns the opaque identity value.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

governed_identity!(ToolInvocationId, "tool invocation");
governed_identity!(InteractionId, "interaction");
governed_identity!(GrantId, "grant");
governed_identity!(ReceiptId, "receipt");
governed_identity!(DispatchAttemptId, "dispatch attempt");

/// Structural construction failure for a portable C5 value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidGovernanceValue(&'static str);

impl fmt::Display for InvalidGovernanceValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} value cannot be empty", self.0)
    }
}

impl Error for InvalidGovernanceValue {}

fn required(value: &str, label: &'static str) -> Result<(), InvalidGovernanceValue> {
    if value.is_empty() {
        Err(InvalidGovernanceValue(label))
    } else {
        Ok(())
    }
}

/// Exact authority grant that may permit one prepared invocation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvocationGrant {
    /// Runtime-owned grant identity.
    pub grant_id: GrantId,
    /// Exact invocation identity.
    pub invocation_id: ToolInvocationId,
    /// Bound C4 digest.
    pub prepared_digest: String,
    /// Bound exact tool name.
    pub tool_name: String,
    /// Bound exact tool revision.
    pub tool_revision: String,
    /// Equal or stricter executor requirements.
    pub granted_requirements: ExecutionRequirements,
    /// Digest of Runtime-owned constraints.
    pub constraints_digest: String,
    /// Exact authority policy revision.
    pub authority_revision: String,
}

impl InvocationGrant {
    /// Rejects empty structural bindings; semantic binding is checked by the reducer.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        grant_id: GrantId,
        invocation_id: ToolInvocationId,
        prepared_digest: impl Into<String>,
        tool_name: impl Into<String>,
        tool_revision: impl Into<String>,
        granted_requirements: ExecutionRequirements,
        constraints_digest: impl Into<String>,
        authority_revision: impl Into<String>,
    ) -> Result<Self, InvalidGovernanceValue> {
        let value = Self {
            grant_id,
            invocation_id,
            prepared_digest: prepared_digest.into(),
            tool_name: tool_name.into(),
            tool_revision: tool_revision.into(),
            granted_requirements,
            constraints_digest: constraints_digest.into(),
            authority_revision: authority_revision.into(),
        };
        required(&value.prepared_digest, "prepared digest")?;
        required(&value.tool_name, "tool name")?;
        required(&value.tool_revision, "tool revision")?;
        required(&value.constraints_digest, "constraints digest")?;
        required(&value.authority_revision, "authority revision")?;
        Ok(value)
    }
}

/// Human/product interaction kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InteractionKind {
    /// Authority approval is required.
    Approval,
    /// Typed external input is required.
    ExternalInput,
}

/// Exact interaction request bound to one invocation and Prepared Call.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InteractionRequest {
    /// Runtime-owned interaction identity.
    pub interaction_id: InteractionId,
    /// Bound invocation identity.
    pub invocation_id: ToolInvocationId,
    /// Bound C4 digest.
    pub prepared_digest: String,
    /// Required interaction family.
    pub kind: InteractionKind,
    /// Redacted structured prompt.
    pub prompt: Value,
    /// Portable response schema.
    pub response_schema: Value,
    /// Stable Runtime-owned expiry policy reference.
    pub expiry_policy: String,
}

impl InteractionRequest {
    /// Rejects empty bindings and prompts outside the public v1 schema.
    pub fn validate(&self) -> Result<(), InvalidGovernanceValue> {
        required(&self.prepared_digest, "prepared digest")?;
        required(&self.expiry_policy, "expiry policy")?;
        let prompt = self
            .prompt
            .as_object()
            .ok_or(InvalidGovernanceValue("interaction prompt"))?;
        let optional = ["message_text", "cancel_label_key"];
        if prompt.len() < 3
            || prompt.len() > 5
            || prompt.get("schema_version").and_then(Value::as_u64) != Some(1)
            || !non_empty_text(prompt.get("title_key"))
            || !non_empty_text(prompt.get("action_label_key"))
            || optional.iter().any(|key| {
                prompt
                    .get(*key)
                    .is_some_and(|value| !non_empty_text(Some(value)))
            })
            || prompt.keys().any(|key| {
                !matches!(
                    key.as_str(),
                    "schema_version"
                        | "title_key"
                        | "message_text"
                        | "action_label_key"
                        | "cancel_label_key"
                )
            })
        {
            return Err(InvalidGovernanceValue("interaction prompt"));
        }
        Ok(())
    }
}

fn non_empty_text(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_str)
        .is_some_and(|text| !text.is_empty())
}

/// Typed continuation fact for one requested interaction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InteractionResolution {
    /// A schema-validated response was durably committed.
    Resolved {
        /// Exact interaction identity.
        interaction_id: InteractionId,
        /// Exact invocation identity.
        invocation_id: ToolInvocationId,
        /// Exact C4 digest.
        prepared_digest: String,
        /// Typed response retained as durable evidence.
        response: Value,
    },
    /// The interaction was durably cancelled.
    Cancelled {
        /// Exact interaction identity.
        interaction_id: InteractionId,
        /// Exact invocation identity.
        invocation_id: ToolInvocationId,
        /// Exact C4 digest.
        prepared_digest: String,
    },
}

/// Trustworthy executor terminal classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalClassification {
    /// Executor proves successful completion.
    Completed,
    /// Executor proves terminal failure.
    Failed,
}

/// Trustworthy receipt binding authority, invocation, executor, and result evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectReceipt {
    /// Runtime-owned receipt identity.
    pub receipt_id: ReceiptId,
    /// Exact invocation identity.
    pub invocation_id: ToolInvocationId,
    /// Exact C4 digest.
    pub prepared_digest: String,
    /// Exact grant identity.
    pub grant_id: GrantId,
    /// Stable executor identity.
    pub executor_id: String,
    /// Exact executor revision.
    pub executor_revision: String,
    /// Proven terminal effect classification.
    pub terminal_classification: TerminalClassification,
    /// Digest of result or terminal evidence.
    pub result_digest: String,
}

impl EffectReceipt {
    /// Rejects empty receipt evidence fields.
    pub fn validate(&self) -> Result<(), InvalidGovernanceValue> {
        required(&self.prepared_digest, "prepared digest")?;
        required(&self.executor_id, "executor identity")?;
        required(&self.executor_revision, "executor revision")?;
        required(&self.result_digest, "result digest")
    }
}

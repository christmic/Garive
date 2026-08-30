//! Reconstruction of one exact Prepared-v3 call from durable recovery state.

use garive_ledger::{CanonicalPayload, TurnSnapshot};
use garive_tools::{PreparedToolCall, ToolAccessResolver, ToolCatalog, ToolIntent};
use serde::Serialize;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

/// Stable fail-closed F0 reconstruction failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum F0RecoveryError {
    /// Fact identity, schema, canonical content or Prepared binding disagrees.
    InvalidBinding,
    /// Referenced argument content cannot be resolved and verified.
    ContentUnavailable,
    /// Recovered arguments exceed the explicit Runtime bound.
    ContentLimitExceeded,
    /// The installed catalogue or trusted access resolver rejects reconstruction.
    PreparationRejected,
}

/// Resolver for opaque Runtime-owned content references.
pub trait F0RecoveryContentPort {
    /// Resolves exact bytes for one reference without interpreting JSON.
    fn resolve(&mut self, reference: &str) -> Result<String, F0RecoveryError>;
}

/// Exact reconstructed call and stable invocation identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveredF0Prepared {
    /// Runtime-owned invocation identity from the durable envelope.
    pub invocation_id: String,
    /// Re-prepared call proven equal to the durable v3 bindings.
    pub prepared: PreparedToolCall,
}

/// Re-prepares one invocation from canonical arguments and verifies every v3 binding.
pub fn recover_f0_prepared(
    turn: &TurnSnapshot,
    invocation_id: &str,
    catalogue: &ToolCatalog,
    access_resolver: &dyn ToolAccessResolver,
    content: &mut dyn F0RecoveryContentPort,
    max_arguments_bytes: usize,
) -> Result<RecoveredF0Prepared, F0RecoveryError> {
    if invocation_id.is_empty() || max_arguments_bytes == 0 {
        return Err(F0RecoveryError::InvalidBinding);
    }
    let fact = turn
        .facts
        .iter()
        .find(|fact| {
            fact.kind.as_str() == "effect.prepared"
                && fact.schema_version == 3
                && fact
                    .tool_invocation_id
                    .as_ref()
                    .is_some_and(|id| id.as_str() == invocation_id)
        })
        .ok_or(F0RecoveryError::InvalidBinding)?;
    let value = payload(fact.payload.as_json())?;
    let arguments = resolve_content(
        value
            .get("arguments")
            .and_then(Value::as_object)
            .ok_or(F0RecoveryError::InvalidBinding)?,
        content,
        max_arguments_bytes,
    )?;
    let prepared = catalogue
        .prepare_v3(
            &ToolIntent::new(
                text(&value, "model_call_id")?,
                text(&value, "tool_name")?,
                arguments,
            ),
            access_resolver,
        )
        .map_err(|_| F0RecoveryError::PreparationRejected)?;
    let access_digest = canonical_digest(
        prepared
            .invocation_accesses()
            .ok_or(F0RecoveryError::InvalidBinding)?,
    )?;
    if prepared.input_digest() != text(&value, "prepared_digest")?
        || prepared.tool_revision() != text(&value, "tool_revision")?
        || prepared.access_policy_revision() != Some(text(&value, "access_policy_revision")?)
        || prepared.access_resolver_revision() != Some(text(&value, "access_resolver_revision")?)
        || prepared.max_result_bytes() != value.get("max_result_bytes").and_then(Value::as_u64)
        || prepared.sandbox_requirements_digest()
            != Some(text(&value, "sandbox_requirements_digest")?)
        || access_digest != content_digest(&value, "invocation_accesses")?
    {
        return Err(F0RecoveryError::InvalidBinding);
    }
    Ok(RecoveredF0Prepared {
        invocation_id: invocation_id.into(),
        prepared,
    })
}

fn resolve_content(
    binding: &Map<String, Value>,
    resolver: &mut dyn F0RecoveryContentPort,
    max_bytes: usize,
) -> Result<String, F0RecoveryError> {
    let digest = text(binding, "digest")?;
    let value = match (
        binding.get("inline_utf8").and_then(Value::as_str),
        binding.get("reference").and_then(Value::as_str),
    ) {
        (Some(inline), None) => inline.into(),
        (None, Some(reference)) if !reference.is_empty() => resolver.resolve(reference)?,
        _ => return Err(F0RecoveryError::InvalidBinding),
    };
    if value.len() > max_bytes {
        return Err(F0RecoveryError::ContentLimitExceeded);
    }
    if format!("{:x}", Sha256::digest(value.as_bytes())) != digest {
        return Err(F0RecoveryError::InvalidBinding);
    }
    Ok(value)
}

fn payload(value: &str) -> Result<Map<String, Value>, F0RecoveryError> {
    serde_json::from_str::<Value>(value)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .ok_or(F0RecoveryError::InvalidBinding)
}

fn text<'a>(value: &'a Map<String, Value>, key: &str) -> Result<&'a str, F0RecoveryError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or(F0RecoveryError::InvalidBinding)
}

fn content_digest<'a>(
    value: &'a Map<String, Value>,
    key: &str,
) -> Result<&'a str, F0RecoveryError> {
    value
        .get(key)
        .and_then(Value::as_object)
        .ok_or(F0RecoveryError::InvalidBinding)
        .and_then(|binding| text(binding, "digest"))
}

fn canonical_digest(value: &impl Serialize) -> Result<String, F0RecoveryError> {
    let value = serde_json::to_value(value).map_err(|_| F0RecoveryError::InvalidBinding)?;
    CanonicalPayload::from_value(&value)
        .map(|payload| payload.sha256().into())
        .map_err(|_| F0RecoveryError::InvalidBinding)
}

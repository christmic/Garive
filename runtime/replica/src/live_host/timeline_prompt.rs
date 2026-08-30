use std::collections::BTreeMap;

use garive_ledger::{CanonicalPayload, DurableFact};
use serde::Deserialize;
use serde_json::Value;

use super::{HostReadLimits, LiveHostError, SuspensionViewV1};

const PROMPT_SCHEMA: &str = "garive.public-suspension-prompt.v1";

pub(super) fn interactions(
    facts: &[DurableFact],
    limits: HostReadLimits,
) -> Result<BTreeMap<String, Interaction>, LiveHostError> {
    let mut values = BTreeMap::new();
    for fact in facts {
        if fact.kind.as_str() != "interaction.requested" {
            continue;
        }
        if fact.schema_version != 1 {
            return Err(LiveHostError::CorruptState);
        }
        let value: Interaction = serde_json::from_str(fact.payload.as_json())
            .map_err(|_| LiveHostError::CorruptState)?;
        verify_json(&value.prompt, limits.max_prompt_bytes)?;
        verify_json(&value.response_schema, limits.max_prompt_bytes)?;
        if value.response_schema.digest != value.response_schema_digest
            || values.insert(value.suspension_id.clone(), value).is_some()
        {
            return Err(LiveHostError::CorruptState);
        }
    }
    Ok(values)
}

pub(super) fn suspension_view(
    suspension_id: &str,
    reason: &str,
    session_version: u64,
    interactions: &BTreeMap<String, Interaction>,
    limits: HostReadLimits,
) -> Result<SuspensionViewV1, LiveHostError> {
    if let Some(request) = interactions.get(suspension_id) {
        let expected = match request.kind.as_str() {
            "approval" => "approval_required",
            "external_input" => "external_input_required",
            _ => return Err(LiveHostError::CorruptState),
        };
        if reason != expected {
            return Err(LiveHostError::CorruptState);
        }
        validate_public_prompt(&request.prompt.inline_utf8)?;
        return Ok(SuspensionViewV1 {
            suspension_id: suspension_id.to_owned(),
            session_version,
            kind: reason.to_owned(),
            prompt_schema: PROMPT_SCHEMA,
            prompt_json: request.prompt.inline_utf8.clone(),
            prompt_digest: request.prompt.digest.clone(),
            response_schema_json: Some(request.response_schema.inline_utf8.clone()),
            response_schema_digest: Some(request.response_schema_digest.clone()),
        });
    }
    let action = match reason {
        "partial_output" => "continue",
        "resource_unavailable" => "retry",
        "operator_reconciliation" => "inspect",
        "delegation_pending" => "wait",
        _ => return Err(LiveHostError::CorruptState),
    };
    let prompt = serde_json::json!({
        "action_label_key":format!("suspension.{reason}.{action}"),
        "schema_version":1,
        "title_key":format!("suspension.{reason}.title"),
    });
    let prompt = CanonicalPayload::from_value(&prompt).map_err(|_| LiveHostError::CorruptState)?;
    if prompt.as_json().len() > limits.max_prompt_bytes {
        return Err(LiveHostError::ReadBoundExceeded);
    }
    Ok(SuspensionViewV1 {
        suspension_id: suspension_id.to_owned(),
        session_version,
        kind: reason.to_owned(),
        prompt_schema: PROMPT_SCHEMA,
        prompt_json: prompt.as_json().to_owned(),
        prompt_digest: prompt.sha256().to_owned(),
        response_schema_json: None,
        response_schema_digest: None,
    })
}

fn validate_public_prompt(input: &str) -> Result<(), LiveHostError> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Prompt {
        schema_version: u64,
        title_key: String,
        message_text: Option<String>,
        action_label_key: String,
        cancel_label_key: Option<String>,
    }
    let value: Prompt = serde_json::from_str(input).map_err(|_| LiveHostError::CorruptState)?;
    if value.schema_version != 1
        || value.title_key.is_empty()
        || value.action_label_key.is_empty()
        || value.message_text.as_deref() == Some("")
        || value.cancel_label_key.as_deref() == Some("")
    {
        return Err(LiveHostError::CorruptState);
    }
    Ok(())
}

fn verify_json(content: &Content, limit: usize) -> Result<(), LiveHostError> {
    if content.inline_utf8.len() > limit {
        return Err(LiveHostError::ReadBoundExceeded);
    }
    let value: Value =
        serde_json::from_str(&content.inline_utf8).map_err(|_| LiveHostError::CorruptState)?;
    let canonical =
        CanonicalPayload::from_value(&value).map_err(|_| LiveHostError::CorruptState)?;
    if canonical.as_json() != content.inline_utf8 || canonical.sha256() != content.digest {
        return Err(LiveHostError::CorruptState);
    }
    Ok(())
}

#[derive(Deserialize)]
pub(super) struct Interaction {
    suspension_id: String,
    kind: String,
    prompt: Content,
    response_schema: Content,
    response_schema_digest: String,
}

#[derive(Deserialize)]
struct Content {
    digest: String,
    inline_utf8: String,
}

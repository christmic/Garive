use std::fmt::Write;

use garive_ledger::CanonicalPayload;
use garive_llm::{
    MediaKind, ModelCapability, ModelInputContent, ModelInputItem, ModelItem, ModelRequest,
    ModelRole, ReasoningContent, TextMode,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::RuntimeCommandError;

/// Computes the C6 v1 digest of one valid frozen provider-neutral request.
pub fn canonical_model_request_digest(
    request: &ModelRequest,
) -> Result<String, RuntimeCommandError> {
    request
        .validate()
        .map_err(|_| RuntimeCommandError::InvalidCommand)?;
    let value = json!({
        "target_id":request.target_id.as_str(),
        "required_capabilities":request.required_capabilities.iter().map(capability).collect::<Vec<_>>(),
        "input_items":request.input_items.iter().map(input_item).collect::<Vec<_>>(),
        "tools":request.tools.iter().map(|tool| json!({
            "name":tool.name,"description":tool.description,
            "definition_revision":tool.definition_revision,
            "input_schema_json":tool.input_schema_json,"strict":tool.strict,
        })).collect::<Vec<_>>(),
        "output":{
            "max_output_tokens":request.output.max_output_tokens,
            "text_mode":text_mode(&request.output.text_mode),
            "reasoning_visibility":request.output.reasoning_visibility,
        },
        "trace_metadata":request.trace_metadata.iter().map(|(key,value)| json!([key,value])).collect::<Vec<_>>(),
    });
    CanonicalPayload::from_value(&value)
        .map(|canonical| canonical.sha256().to_owned())
        .map_err(|_| RuntimeCommandError::InvariantViolation)
}

fn capability(value: &ModelCapability) -> &'static str {
    match value {
        ModelCapability::Text => "text",
        ModelCapability::Vision => "vision",
        ModelCapability::Reasoning => "reasoning",
        ModelCapability::Tools => "tools",
        ModelCapability::JsonOutput => "json_output",
        ModelCapability::Streaming => "streaming",
    }
}

fn input_item(value: &ModelInputItem) -> Value {
    match value {
        ModelInputItem::Message { role, content } => json!({
            "kind":"message","role":role_value(*role),
            "content":content.iter().map(input_content).collect::<Vec<_>>(),
        }),
        ModelInputItem::ToolObservation {
            model_call_id,
            result_json,
        } => {
            json!({"kind":"tool_observation","model_call_id":model_call_id,"result_json":result_json})
        }
        ModelInputItem::ReasoningReference { reference } => {
            json!({"kind":"reasoning_reference","reference":reference})
        }
    }
}

fn input_content(value: &ModelInputContent) -> Value {
    match value {
        ModelInputContent::Text(text) => json!({"kind":"text","text":text}),
        ModelInputContent::MediaReference {
            media_kind,
            reference,
            media_type,
        } => json!({
            "kind":"media_reference","media_kind":media_kind_value(media_kind),
            "reference":reference,"media_type":media_type,
        }),
    }
}

const fn role_value(value: ModelRole) -> &'static str {
    match value {
        ModelRole::System => "system",
        ModelRole::Developer => "developer",
        ModelRole::User => "user",
        ModelRole::Assistant => "assistant",
    }
}

fn text_mode(value: &TextMode) -> Value {
    match value {
        TextMode::Plain => json!({"kind":"plain"}),
        TextMode::JsonObject => json!({"kind":"json_object"}),
        TextMode::JsonSchema { schema_json } => {
            json!({"kind":"json_schema","schema_json":schema_json})
        }
    }
}

pub(super) fn content(items: &[ModelItem]) -> Result<Value, RuntimeCommandError> {
    let values = items.iter().map(model_item).collect::<Vec<_>>();
    let canonical = CanonicalPayload::from_value(&Value::Array(values))
        .map_err(|_| RuntimeCommandError::InvariantViolation)?;
    Ok(json!({"digest":canonical.sha256(),"inline_utf8":canonical.as_json()}))
}

pub(super) fn text_content(value: &str) -> Result<Value, RuntimeCommandError> {
    let digest = digest(value.as_bytes());
    Ok(json!({"digest":digest,"inline_utf8":value}))
}

fn model_item(item: &ModelItem) -> Value {
    match item {
        ModelItem::Text { text } => json!({"kind":"text","text":text}),
        ModelItem::Refusal { text } => json!({"kind":"refusal","text":text}),
        ModelItem::Reasoning { content } => match content {
            ReasoningContent::ModelVisible(text) => {
                json!({"kind":"reasoning","visibility":"model_visible","value":text})
            }
            ReasoningContent::OpaqueReference(reference) => {
                json!({"kind":"reasoning","visibility":"opaque_reference","value":reference})
            }
        },
        ModelItem::ToolIntent {
            model_call_id,
            tool_name,
            arguments_json,
        } => {
            json!({"kind":"tool_intent","model_call_id":model_call_id,"tool_name":tool_name,"arguments_json":arguments_json})
        }
        ModelItem::ToolObservation {
            model_call_id,
            result_json,
        } => {
            json!({"kind":"tool_observation","model_call_id":model_call_id,"result_json":result_json})
        }
        ModelItem::MediaReference {
            media_kind,
            reference,
        } => {
            json!({"kind":"media_reference","media_kind":media_kind_value(media_kind),"reference":reference})
        }
    }
}

pub(super) fn media_kind_value(kind: &MediaKind) -> Value {
    match kind {
        MediaKind::Image => json!("image"),
        MediaKind::Audio => json!("audio"),
        MediaKind::Video => json!("video"),
        MediaKind::File => json!("file"),
        MediaKind::Other(value) => json!({"other":value}),
    }
}

pub(super) fn digest(value: &[u8]) -> String {
    let mut output = String::with_capacity(64);
    for byte in Sha256::digest(value) {
        write!(&mut output, "{byte:02x}").unwrap();
    }
    output
}

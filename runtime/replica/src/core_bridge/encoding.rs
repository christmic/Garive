use std::fmt::Write;

use garive_ledger::CanonicalPayload;
use garive_llm::{MediaKind, ModelItem, ReasoningContent};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::RuntimeCommandError;

pub(super) fn content(items: &[ModelItem]) -> Result<Value, RuntimeCommandError> {
    let values = items.iter().map(model_item).collect::<Vec<_>>();
    let canonical = CanonicalPayload::from_value(&Value::Array(values))
        .map_err(|_| RuntimeCommandError::InvariantViolation)?;
    Ok(json!({"digest":canonical.sha256(),"inline_utf8":canonical.as_json()}))
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

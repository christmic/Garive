use garive_anthropic_messages as messages;
use serde_json::{json, Value};

use crate::CompatibleProviderError;

const THINKING_KIND: &str = "anthropic.messages.thinking.v1";
const REDACTED_KIND: &str = "anthropic.messages.redacted-thinking.v1";

pub(crate) fn encode_thinking(
    thinking: &str,
    signature: &str,
) -> Result<String, CompatibleProviderError> {
    if thinking.is_empty() || signature.is_empty() {
        return Err(CompatibleProviderError::ProtocolInvariant);
    }
    serde_json::to_string(&json!({
        "kind": THINKING_KIND,
        "thinking": thinking,
        "signature": signature,
    }))
    .map_err(|_| CompatibleProviderError::ProtocolInvariant)
}

pub(crate) fn encode_redacted(data: &str) -> Result<String, CompatibleProviderError> {
    if data.is_empty() {
        return Err(CompatibleProviderError::ProtocolInvariant);
    }
    serde_json::to_string(&json!({"kind": REDACTED_KIND, "data": data}))
        .map_err(|_| CompatibleProviderError::ProtocolInvariant)
}

pub(crate) fn decode(reference: &str) -> Result<messages::ContentBlock, CompatibleProviderError> {
    let Value::Object(reference) =
        serde_json::from_str(reference).map_err(|_| CompatibleProviderError::UnsupportedInput)?
    else {
        return Err(CompatibleProviderError::UnsupportedInput);
    };
    match reference.get("kind").and_then(Value::as_str) {
        Some(THINKING_KIND) if reference.len() == 3 => {
            let thinking = nonempty(&reference, "thinking")?;
            let signature = nonempty(&reference, "signature")?;
            Ok(messages::ContentBlock::Thinking {
                thinking: thinking.to_owned(),
                signature: signature.to_owned(),
            })
        }
        Some(REDACTED_KIND) if reference.len() == 2 => {
            Ok(messages::ContentBlock::RedactedThinking {
                data: nonempty(&reference, "data")?.to_owned(),
            })
        }
        _ => Err(CompatibleProviderError::UnsupportedInput),
    }
}

fn nonempty<'a>(
    reference: &'a serde_json::Map<String, Value>,
    key: &str,
) -> Result<&'a str, CompatibleProviderError> {
    reference
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or(CompatibleProviderError::UnsupportedInput)
}

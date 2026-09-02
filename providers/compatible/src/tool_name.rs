//! Collision-resistant mapping between neutral and protocol-safe tool names.

use std::collections::BTreeMap;

use garive_llm::{InvokeOutcome, ModelItem, ToolDescriptor};
use sha2::{Digest, Sha256};

use crate::CompatibleProviderError;

const MAX_PORTABLE_WIRE_NAME_BYTES: usize = 64;

/// Maps one neutral tool identity into the shared Responses/Messages name grammar.
pub fn wire_tool_name(name: &str) -> String {
    if !name.is_empty()
        && name.len() <= MAX_PORTABLE_WIRE_NAME_BYTES
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return name.to_owned();
    }
    let digest = format!("{:x}", Sha256::digest(name.as_bytes()));
    format!("garive_{}", &digest[..MAX_PORTABLE_WIRE_NAME_BYTES - 7])
}

/// Restores tool intents to the exact neutral names frozen in the request.
pub fn restore_neutral_tool_names(
    outcome: &mut InvokeOutcome,
    tools: &[ToolDescriptor],
) -> Result<(), CompatibleProviderError> {
    let mut names = BTreeMap::new();
    for tool in tools {
        if names
            .insert(wire_tool_name(&tool.name), tool.name.as_str())
            .is_some()
        {
            return Err(CompatibleProviderError::ProtocolInvariant);
        }
    }
    let items = match outcome {
        InvokeOutcome::Completed { items, .. } => items,
        InvokeOutcome::Interrupted { partial_items, .. } => partial_items,
        _ => return Ok(()),
    };
    for item in items {
        if let ModelItem::ToolIntent { tool_name, .. } = item {
            *tool_name = names
                .get(tool_name)
                .ok_or(CompatibleProviderError::ProtocolInvariant)?
                .to_string();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use garive_llm::{ModelStopReason, ModelUsage, TokenCount, UsageSource};

    use super::*;

    #[test]
    fn dotted_neutral_name_round_trips_through_safe_wire_identity() {
        let neutral = "garive.workspace.read_text";
        let wire = wire_tool_name(neutral);
        assert_ne!(wire, neutral);
        assert!(wire
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-')));
        let tools = vec![ToolDescriptor {
            name: neutral.into(),
            definition_revision: "1".into(),
            description: "read".into(),
            input_schema_json: "{}".into(),
            strict: true,
        }];
        let mut outcome = InvokeOutcome::Completed {
            stop_reason: ModelStopReason::ToolUse,
            items: vec![ModelItem::ToolIntent {
                model_call_id: "call-1".into(),
                tool_name: wire,
                arguments_json: "{}".into(),
            }],
            usage: ModelUsage {
                input_tokens: TokenCount::Unknown,
                output_tokens: TokenCount::Unknown,
                cache_read_tokens: None,
                cache_write_tokens: None,
                source: UsageSource::Estimated,
            },
        };
        restore_neutral_tool_names(&mut outcome, &tools).unwrap();
        let InvokeOutcome::Completed { items, .. } = outcome else {
            panic!("completed outcome")
        };
        assert!(matches!(
            &items[0],
            ModelItem::ToolIntent { tool_name, .. } if tool_name == neutral
        ));
    }
}

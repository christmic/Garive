use std::time::Duration;

use garive_anthropic_messages as messages;
use garive_llm::{
    InterruptionKind, InvokeOutcome, ModelItem, ModelStopReason, ModelUsage, ReasoningContent,
    TokenCount, UsageSource,
};
use garive_openai_responses as responses;

use crate::{CompatibleProviderError, ErrorDisposition, ErrorSignature, ProtocolErrorPolicy};

/// Classifies an exact typed protocol error without inspecting its message.
pub fn classify_protocol_error(
    policy: &ProtocolErrorPolicy,
    signature: ErrorSignature,
    retry_after: Option<Duration>,
) -> Result<InvokeOutcome, CompatibleProviderError> {
    match policy.classify(&signature) {
        Some(ErrorDisposition::Rejected(kind)) => Ok(InvokeOutcome::Rejected {
            kind,
            sanitized_evidence: format!(
                "status={};type={};code={}",
                signature.status,
                signature.protocol_type,
                signature.code.as_deref().unwrap_or("")
            ),
        }),
        Some(ErrorDisposition::Unavailable(kind)) => {
            Ok(InvokeOutcome::Unavailable { kind, retry_after })
        }
        Some(ErrorDisposition::Interrupted(kind)) => Ok(InvokeOutcome::Interrupted {
            kind,
            partial_items: Vec::new(),
            usage: unknown_usage(),
        }),
        None => Err(CompatibleProviderError::UnclassifiedProtocolError),
    }
}

/// Normalizes one adapter-validated buffered Responses terminal.
pub fn normalize_responses(
    response: &responses::Response,
    reasoning_visibility: bool,
) -> Result<InvokeOutcome, CompatibleProviderError> {
    let items = responses_items(&response.output, reasoning_visibility)?;
    let usage = response
        .usage
        .as_ref()
        .map(responses_usage)
        .unwrap_or_else(unknown_usage);
    match response.status {
        Some(responses::ResponseStatus::Completed) if response.error.is_none() => {
            Ok(InvokeOutcome::Completed {
                stop_reason: inferred_stop(&items),
                items,
                usage,
            })
        }
        Some(responses::ResponseStatus::Incomplete)
            if response.error.is_none()
                && response
                    .incomplete_details
                    .as_ref()
                    .map(|value| value.reason.as_str())
                    == Some("max_output_tokens") =>
        {
            Ok(InvokeOutcome::Interrupted {
                kind: InterruptionKind::OutputLimit,
                partial_items: items,
                usage,
            })
        }
        Some(responses::ResponseStatus::Cancelled) if response.error.is_none() => {
            Ok(InvokeOutcome::Interrupted {
                kind: InterruptionKind::Cancelled,
                partial_items: items,
                usage,
            })
        }
        _ => Err(CompatibleProviderError::ProtocolInvariant),
    }
}

/// Normalizes one adapter-validated buffered Messages terminal.
pub fn normalize_messages(
    response: &messages::MessageResponse,
    reasoning_visibility: bool,
) -> Result<InvokeOutcome, CompatibleProviderError> {
    let items = messages_items(
        &response.content,
        reasoning_visibility,
        response.stop_reason == Some(messages::StopReason::Refusal),
    )?;
    let usage = messages_usage(&response.usage);
    match response.stop_reason {
        Some(messages::StopReason::MaxTokens) => Ok(InvokeOutcome::Interrupted {
            kind: InterruptionKind::OutputLimit,
            partial_items: items,
            usage,
        }),
        Some(messages::StopReason::ModelContextWindowExceeded) => Ok(InvokeOutcome::Rejected {
            kind: garive_llm::RejectionKind::ContextOverflow,
            sanitized_evidence: "model_context_window_exceeded".to_owned(),
        }),
        Some(reason) => Ok(InvokeOutcome::Completed {
            items,
            usage,
            stop_reason: messages_stop(reason),
        }),
        None => Err(CompatibleProviderError::ProtocolInvariant),
    }
}

fn responses_items(
    output: &[responses::ResponseOutputItem],
    reasoning_visibility: bool,
) -> Result<Vec<ModelItem>, CompatibleProviderError> {
    let mut items = Vec::new();
    for item in output {
        match item {
            responses::ResponseOutputItem::Message(message) => {
                for content in &message.content {
                    items.push(match content {
                        responses::OutputContent::OutputText(value) => ModelItem::Text {
                            text: value.text.clone(),
                        },
                        responses::OutputContent::Refusal(value) => ModelItem::Refusal {
                            text: value.refusal.clone(),
                        },
                        responses::OutputContent::Extension(_) => {
                            return Err(CompatibleProviderError::UnsupportedExtension)
                        }
                    });
                }
            }
            responses::ResponseOutputItem::FunctionCall(call) => {
                let arguments = canonical_object(&call.arguments)?;
                items.push(ModelItem::ToolIntent {
                    model_call_id: call.call_id.clone(),
                    tool_name: call.name.clone(),
                    arguments_json: arguments,
                });
            }
            responses::ResponseOutputItem::Reasoning(reasoning) => {
                if reasoning_visibility {
                    let parts = reasoning.content.as_ref().unwrap_or(&reasoning.summary);
                    for part in parts {
                        items.push(ModelItem::Reasoning {
                            content: ReasoningContent::ModelVisible(part.text.clone()),
                        });
                    }
                } else if let Some(reference) = &reasoning.encrypted_content {
                    items.push(ModelItem::Reasoning {
                        content: ReasoningContent::OpaqueReference(reference.clone()),
                    });
                }
            }
            responses::ResponseOutputItem::Extension(_) => {
                return Err(CompatibleProviderError::UnsupportedExtension)
            }
        }
    }
    Ok(items)
}

fn messages_items(
    content: &[messages::OutputBlock],
    reasoning_visibility: bool,
    refusal: bool,
) -> Result<Vec<ModelItem>, CompatibleProviderError> {
    content
        .iter()
        .map(|block| match block {
            messages::OutputBlock::Text(value) if refusal => Ok(ModelItem::Refusal {
                text: value.text.clone(),
            }),
            messages::OutputBlock::Text(value) => Ok(ModelItem::Text {
                text: value.text.clone(),
            }),
            messages::OutputBlock::Thinking(value) if reasoning_visibility => {
                Ok(ModelItem::Reasoning {
                    content: ReasoningContent::ModelVisible(value.thinking.clone()),
                })
            }
            messages::OutputBlock::Thinking(value) => Ok(ModelItem::Reasoning {
                content: ReasoningContent::OpaqueReference(value.signature.clone()),
            }),
            messages::OutputBlock::RedactedThinking(value) => Ok(ModelItem::Reasoning {
                content: ReasoningContent::OpaqueReference(value.data.clone()),
            }),
            messages::OutputBlock::ToolUse(value) => Ok(ModelItem::ToolIntent {
                model_call_id: value.id.clone(),
                tool_name: value.name.clone(),
                arguments_json: serde_json::to_string(&value.input)
                    .map_err(|_| CompatibleProviderError::ProtocolInvariant)?,
            }),
            messages::OutputBlock::Extension(_) => {
                Err(CompatibleProviderError::UnsupportedExtension)
            }
        })
        .collect()
}

fn canonical_object(encoded: &str) -> Result<String, CompatibleProviderError> {
    let value: serde_json::Value =
        serde_json::from_str(encoded).map_err(|_| CompatibleProviderError::ProtocolInvariant)?;
    let serde_json::Value::Object(object) = value else {
        return Err(CompatibleProviderError::ProtocolInvariant);
    };
    serde_json::to_string(&object).map_err(|_| CompatibleProviderError::ProtocolInvariant)
}

fn inferred_stop(items: &[ModelItem]) -> ModelStopReason {
    if items
        .iter()
        .any(|item| matches!(item, ModelItem::ToolIntent { .. }))
    {
        ModelStopReason::ToolUse
    } else if items
        .iter()
        .any(|item| matches!(item, ModelItem::Refusal { .. }))
    {
        ModelStopReason::Refusal
    } else {
        ModelStopReason::EndTurn
    }
}

fn messages_stop(reason: messages::StopReason) -> ModelStopReason {
    match reason {
        messages::StopReason::EndTurn => ModelStopReason::EndTurn,
        messages::StopReason::StopSequence => ModelStopReason::StopSequence,
        messages::StopReason::ToolUse => ModelStopReason::ToolUse,
        messages::StopReason::PauseTurn => ModelStopReason::PauseTurn,
        messages::StopReason::Refusal => ModelStopReason::Refusal,
        messages::StopReason::MaxTokens | messages::StopReason::ModelContextWindowExceeded => {
            unreachable!("handled before completed terminal")
        }
    }
}

fn responses_usage(value: &responses::ResponseUsage) -> ModelUsage {
    ModelUsage {
        input_tokens: TokenCount::Known(value.input_tokens),
        output_tokens: TokenCount::Known(value.output_tokens),
        cache_read_tokens: Some(TokenCount::Known(value.input_tokens_details.cached_tokens)),
        cache_write_tokens: Some(TokenCount::Known(
            value.input_tokens_details.cache_write_tokens,
        )),
        source: UsageSource::ProviderReported,
    }
}

fn messages_usage(value: &messages::Usage) -> ModelUsage {
    ModelUsage {
        input_tokens: TokenCount::Known(value.input_tokens),
        output_tokens: TokenCount::Known(value.output_tokens),
        cache_read_tokens: value.cache_read_input_tokens.map(TokenCount::Known),
        cache_write_tokens: value.cache_creation_input_tokens.map(TokenCount::Known),
        source: UsageSource::ProviderReported,
    }
}

fn unknown_usage() -> ModelUsage {
    ModelUsage {
        input_tokens: TokenCount::Unknown,
        output_tokens: TokenCount::Unknown,
        cache_read_tokens: None,
        cache_write_tokens: None,
        source: UsageSource::ProviderReported,
    }
}

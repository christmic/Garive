use garive_anthropic_messages as messages;
use garive_llm::{
    MediaKind, ModelCapability, ModelInputContent, ModelInputItem, ModelRequest, ModelRole,
    TextMode, ToolDescriptor,
};
use garive_openai_responses as responses;
use serde_json::{Map, Value};

use crate::{
    wire_tool_name, CompatibleProviderError, MessagesDeployment, MessagesMediaBinding,
    ResponsesDeployment, ResponsesMediaBinding,
};

/// Maps one neutral request into the portable Responses protocol shape.
pub fn map_responses_request(
    deployment: &ResponsesDeployment,
    request: &ModelRequest,
) -> Result<responses::CreateResponseRequest, CompatibleProviderError> {
    admit_request(&deployment.target_id, &deployment.capabilities, request)?;
    if request.output.reasoning_visibility && deployment.reasoning.is_none() {
        return Err(CompatibleProviderError::UnsupportedCapability(
            ModelCapability::Reasoning,
        ));
    }

    let mut items = Vec::with_capacity(request.input_items.len());
    for item in &request.input_items {
        items.push(match item {
            ModelInputItem::Message { role, content } => responses::InputItem::Message {
                role: responses_role(*role),
                content: content
                    .iter()
                    .map(|part| responses_content(deployment, part))
                    .collect::<Result<_, _>>()?,
            },
            ModelInputItem::ToolIntent {
                model_call_id,
                tool_name,
                arguments_json,
            } => responses::InputItem::FunctionCall(responses::FunctionCall {
                call_id: model_call_id.clone(),
                name: wire_tool_name(tool_name),
                arguments: arguments_json.clone(),
            }),
            ModelInputItem::ToolObservation {
                model_call_id,
                result_json,
            } => responses::InputItem::FunctionCallOutput(responses::FunctionCallOutput {
                call_id: model_call_id.clone(),
                output: responses::FunctionOutput::Text(result_json.clone()),
                status: Some(responses::ItemStatus::Completed),
            }),
            ModelInputItem::ReasoningReference { .. } => {
                return Err(CompatibleProviderError::UnsupportedInput)
            }
        });
    }

    let mut mapped = responses::CreateResponseRequest::new(
        deployment.model_id.clone(),
        responses::ResponseInput::Items(items),
        requires(request, ModelCapability::Streaming),
    );
    mapped.max_output_tokens = request
        .output
        .max_output_tokens
        .or(deployment.default_max_output_tokens);
    mapped.tools = request
        .tools
        .iter()
        .map(responses_tool)
        .collect::<Result<_, _>>()?;
    mapped.tool_choice = (!mapped.tools.is_empty())
        .then_some(responses::ToolChoice::Mode(responses::ToolChoiceMode::Auto));
    mapped.text = Some(responses::ResponseTextConfig {
        format: responses_text_mode(&request.output.text_mode)?,
    });
    mapped.reasoning = deployment.reasoning.clone();
    mapped.metadata = request.trace_metadata.iter().cloned().collect();
    mapped
        .validate()
        .map_err(|_| CompatibleProviderError::InvalidProtocolRequest)?;
    Ok(mapped)
}

/// Maps one neutral request into the portable Messages protocol shape.
pub fn map_messages_request(
    deployment: &MessagesDeployment,
    request: &ModelRequest,
) -> Result<messages::CreateMessageRequest, CompatibleProviderError> {
    admit_request(&deployment.target_id, &deployment.capabilities, request)?;
    if request
        .trace_metadata
        .iter()
        .any(|(key, _)| !matches!(key.as_str(), "turn_id" | "execution_id"))
    {
        return Err(CompatibleProviderError::UnsupportedMetadata);
    }
    if request.output.reasoning_visibility && deployment.thinking.is_none() {
        return Err(CompatibleProviderError::UnsupportedCapability(
            ModelCapability::Reasoning,
        ));
    }
    let max_tokens = request
        .output
        .max_output_tokens
        .or(deployment.default_max_output_tokens)
        .ok_or(CompatibleProviderError::MissingOutputLimit)?;

    let mut system = Vec::new();
    let mut turns = Vec::new();
    let mut conversation_started = false;
    for item in &request.input_items {
        match item {
            ModelInputItem::Message {
                role: ModelRole::System | ModelRole::Developer,
                content,
            } => {
                if conversation_started {
                    return Err(CompatibleProviderError::UnsupportedInput);
                }
                for part in content {
                    let ModelInputContent::Text(text) = part else {
                        return Err(CompatibleProviderError::UnsupportedInput);
                    };
                    system.push(messages::TextBlock {
                        kind: messages::TextBlockType::Text,
                        text: text.clone(),
                        cache_control: None,
                        citations: None,
                    });
                }
            }
            ModelInputItem::Message { role, content } => {
                conversation_started = true;
                turns.push(messages::Message::new(
                    messages_role(*role)?,
                    messages::MessageContent::Blocks(
                        content
                            .iter()
                            .map(|part| messages_content(deployment, part))
                            .collect::<Result<_, _>>()?,
                    ),
                ));
            }
            ModelInputItem::ToolIntent {
                model_call_id,
                tool_name,
                arguments_json,
            } => {
                conversation_started = true;
                push_message_block(
                    &mut turns,
                    messages::MessageRole::Assistant,
                    messages::ContentBlock::ToolUse {
                        id: model_call_id.clone(),
                        name: wire_tool_name(tool_name),
                        input: json_object(arguments_json)?,
                        cache_control: None,
                    },
                );
            }
            ModelInputItem::ToolObservation {
                model_call_id,
                result_json,
            } => {
                conversation_started = true;
                push_message_block(
                    &mut turns,
                    messages::MessageRole::User,
                    messages::ContentBlock::ToolResult {
                        tool_use_id: model_call_id.clone(),
                        content: Some(messages::ToolResultContent::Text(result_json.clone())),
                        is_error: None,
                        cache_control: None,
                    },
                );
            }
            ModelInputItem::ReasoningReference { .. } => {
                return Err(CompatibleProviderError::UnsupportedInput)
            }
        }
    }

    let mut mapped = messages::CreateMessageRequest::new(
        deployment.model_id.clone(),
        max_tokens,
        turns,
        requires(request, ModelCapability::Streaming),
    );
    mapped.system = (!system.is_empty()).then_some(messages::SystemPrompt::Blocks(system));
    mapped.tools = request
        .tools
        .iter()
        .map(messages_tool)
        .collect::<Result<_, _>>()?;
    mapped.tool_choice = (!mapped.tools.is_empty()).then_some(messages::ToolChoice::Auto {
        disable_parallel_tool_use: None,
    });
    mapped.output_config = messages_output(&request.output.text_mode)?;
    mapped.thinking = deployment.thinking.clone();
    mapped
        .validate()
        .map_err(|_| CompatibleProviderError::InvalidProtocolRequest)?;
    Ok(mapped)
}

fn push_message_block(
    turns: &mut Vec<messages::Message>,
    role: messages::MessageRole,
    block: messages::ContentBlock,
) {
    if let Some(messages::Message {
        role: previous_role,
        content: messages::MessageContent::Blocks(blocks),
    }) = turns.last_mut()
    {
        let may_join = match &block {
            messages::ContentBlock::ToolResult { .. } => blocks
                .iter()
                .all(|value| matches!(value, messages::ContentBlock::ToolResult { .. })),
            _ => true,
        };
        if *previous_role == role && may_join {
            blocks.push(block);
            return;
        }
    }
    turns.push(messages::Message::new(
        role,
        messages::MessageContent::Blocks(vec![block]),
    ));
}

fn admit_request(
    target_id: &str,
    capabilities: &std::collections::BTreeSet<ModelCapability>,
    request: &ModelRequest,
) -> Result<(), CompatibleProviderError> {
    request
        .validate()
        .map_err(CompatibleProviderError::InvalidRequest)?;
    if request.target_id.as_str() != target_id {
        return Err(CompatibleProviderError::TargetMismatch);
    }
    if let Some(capability) = request
        .required_capabilities
        .iter()
        .find(|capability| !capabilities.contains(capability))
    {
        return Err(CompatibleProviderError::UnsupportedCapability(
            capability.clone(),
        ));
    }
    Ok(())
}

fn requires(request: &ModelRequest, capability: ModelCapability) -> bool {
    request.required_capabilities.contains(&capability)
}

fn json_object(encoded: &str) -> Result<Map<String, Value>, CompatibleProviderError> {
    match serde_json::from_str(encoded) {
        Ok(Value::Object(object)) => Ok(object),
        _ => Err(CompatibleProviderError::InvalidJsonObject),
    }
}

fn responses_role(role: ModelRole) -> responses::MessageRole {
    match role {
        ModelRole::System => responses::MessageRole::System,
        ModelRole::Developer => responses::MessageRole::Developer,
        ModelRole::User => responses::MessageRole::User,
        ModelRole::Assistant => responses::MessageRole::Assistant,
    }
}

fn responses_content(
    deployment: &ResponsesDeployment,
    content: &ModelInputContent,
) -> Result<responses::InputContent, CompatibleProviderError> {
    match content {
        ModelInputContent::Text(text) => {
            Ok(responses::InputContent::InputText { text: text.clone() })
        }
        ModelInputContent::MediaReference {
            media_kind: MediaKind::Image,
            reference,
            ..
        } => match deployment.media_bindings.get(reference) {
            Some(ResponsesMediaBinding::Url { value, detail }) => {
                Ok(responses::InputContent::InputImage {
                    image_url: Some(value.clone()),
                    file_id: None,
                    detail: *detail,
                })
            }
            Some(ResponsesMediaBinding::FileId { value, detail }) => {
                Ok(responses::InputContent::InputImage {
                    image_url: None,
                    file_id: Some(value.clone()),
                    detail: *detail,
                })
            }
            None => Err(CompatibleProviderError::MissingMediaBinding),
        },
        ModelInputContent::MediaReference { .. } => Err(CompatibleProviderError::UnsupportedInput),
    }
}

fn responses_tool(
    tool: &ToolDescriptor,
) -> Result<responses::ResponseTool, CompatibleProviderError> {
    Ok(responses::ResponseTool::Function(responses::FunctionTool {
        name: wire_tool_name(&tool.name),
        description: Some(tool.description.clone()),
        parameters: json_object(&tool.input_schema_json)?,
        strict: tool.strict,
    }))
}

fn responses_text_mode(mode: &TextMode) -> Result<responses::TextFormat, CompatibleProviderError> {
    Ok(match mode {
        TextMode::Plain => responses::TextFormat::Text,
        TextMode::JsonObject => responses::TextFormat::JsonObject,
        TextMode::JsonSchema { schema_json } => responses::TextFormat::JsonSchema {
            name: "garive_output".to_owned(),
            description: None,
            schema: json_object(schema_json)?,
            strict: true,
        },
    })
}

fn messages_role(role: ModelRole) -> Result<messages::MessageRole, CompatibleProviderError> {
    match role {
        ModelRole::User => Ok(messages::MessageRole::User),
        ModelRole::Assistant => Ok(messages::MessageRole::Assistant),
        ModelRole::System | ModelRole::Developer => Err(CompatibleProviderError::UnsupportedInput),
    }
}

fn messages_content(
    deployment: &MessagesDeployment,
    content: &ModelInputContent,
) -> Result<messages::ContentBlock, CompatibleProviderError> {
    match content {
        ModelInputContent::Text(text) => Ok(messages::ContentBlock::Text {
            text: text.clone(),
            cache_control: None,
        }),
        ModelInputContent::MediaReference { reference, .. } => {
            match deployment.media_bindings.get(reference) {
                Some(MessagesMediaBinding::Image(source)) => Ok(messages::ContentBlock::Image {
                    source: source.clone(),
                    cache_control: None,
                }),
                Some(MessagesMediaBinding::Document(source)) => {
                    Ok(messages::ContentBlock::Document {
                        source: source.clone(),
                        cache_control: None,
                        citations: None,
                        title: None,
                        context: None,
                    })
                }
                None => Err(CompatibleProviderError::MissingMediaBinding),
            }
        }
    }
}

fn messages_tool(tool: &ToolDescriptor) -> Result<messages::Tool, CompatibleProviderError> {
    Ok(messages::Tool {
        name: wire_tool_name(&tool.name),
        input_schema: json_object(&tool.input_schema_json)?,
        description: Some(tool.description.clone()),
        strict: Some(tool.strict),
        cache_control: None,
    })
}

fn messages_output(
    mode: &TextMode,
) -> Result<Option<messages::OutputConfig>, CompatibleProviderError> {
    let schema = match mode {
        TextMode::Plain => return Ok(None),
        TextMode::JsonObject => {
            Map::from_iter([("type".to_owned(), Value::String("object".into()))])
        }
        TextMode::JsonSchema { schema_json } => json_object(schema_json)?,
    };
    Ok(Some(messages::OutputConfig {
        effort: None,
        format: Some(messages::JsonOutputFormat {
            kind: messages::JsonOutputFormatType::JsonSchema,
            schema,
        }),
    }))
}

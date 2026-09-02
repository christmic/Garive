package com.garive.eng.kt.provider.compatible

import com.garive.eng.kt.anthropic.ContentBlock as MessagesContentBlock
import com.garive.eng.kt.anthropic.CreateMessageRequest
import com.garive.eng.kt.anthropic.JsonOutputFormat
import com.garive.eng.kt.anthropic.Message
import com.garive.eng.kt.anthropic.MessageContent
import com.garive.eng.kt.anthropic.MessageRole as MessagesRole
import com.garive.eng.kt.anthropic.OutputConfig
import com.garive.eng.kt.anthropic.SystemPrompt
import com.garive.eng.kt.anthropic.Tool as MessagesTool
import com.garive.eng.kt.anthropic.ToolChoice as MessagesToolChoice
import com.garive.eng.kt.anthropic.ToolResultContent
import com.garive.eng.kt.llm.MediaKind
import com.garive.eng.kt.llm.ModelCapability
import com.garive.eng.kt.llm.ModelInputContent
import com.garive.eng.kt.llm.ModelInputItem
import com.garive.eng.kt.llm.ModelRequest
import com.garive.eng.kt.llm.ModelRole
import com.garive.eng.kt.llm.TextMode
import com.garive.eng.kt.llm.ToolDescriptor
import com.garive.eng.kt.openai.CreateResponseRequest
import com.garive.eng.kt.openai.FunctionOutput
import com.garive.eng.kt.openai.FunctionTool
import com.garive.eng.kt.openai.InputContent as ResponsesContent
import com.garive.eng.kt.openai.InputItem as ResponsesItem
import com.garive.eng.kt.openai.ItemStatus
import com.garive.eng.kt.openai.MessageRole as ResponsesRole
import com.garive.eng.kt.openai.ResponseInput
import com.garive.eng.kt.openai.ResponseTextConfig
import com.garive.eng.kt.openai.TextFormat
import com.garive.eng.kt.openai.ToolChoice as ResponsesToolChoice
import com.garive.eng.kt.openai.ToolChoiceMode
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

/** Maps one neutral request into the portable Responses protocol shape. */
public fun mapResponsesRequest(deployment: ResponsesDeployment, request: ModelRequest): CreateResponseRequest {
    admit(deployment.targetId, deployment.capabilities, request)
    if (request.output.reasoningVisibility && deployment.reasoning == null) {
        fail(CompatibleProviderError.UNSUPPORTED_CAPABILITY, capability = ModelCapability.REASONING)
    }
    val mapped = CreateResponseRequest(
        model = deployment.modelId,
        input = ResponseInput.Items(request.inputItems.map { item ->
            when (item) {
                is ModelInputItem.Message -> ResponsesItem.Message(
                    role = when (item.role) {
                        ModelRole.SYSTEM -> ResponsesRole.SYSTEM
                        ModelRole.DEVELOPER -> ResponsesRole.DEVELOPER
                        ModelRole.USER -> ResponsesRole.USER
                        ModelRole.ASSISTANT -> ResponsesRole.ASSISTANT
                    },
                    content = item.content.map { responsesContent(deployment, it) },
                )
                is ModelInputItem.ToolIntent -> ResponsesItem.FunctionCall(
                    callId = item.modelCallId,
                    name = wireToolName(item.toolName),
                    arguments = item.argumentsJson,
                )
                is ModelInputItem.ToolObservation -> ResponsesItem.FunctionCallOutput(
                    callId = item.modelCallId,
                    output = FunctionOutput.Text(item.resultJson),
                    status = ItemStatus.COMPLETED,
                )
                is ModelInputItem.ReasoningReference -> fail(CompatibleProviderError.UNSUPPORTED_INPUT)
            }
        }),
        stream = ModelCapability.STREAMING in request.requiredCapabilities,
        maxOutputTokens = request.output.maxOutputTokens ?: deployment.defaultMaxOutputTokens,
        tools = request.tools.map(::responsesTool),
        toolChoice = request.tools.takeIf(List<ToolDescriptor>::isNotEmpty)?.let {
            ResponsesToolChoice.Mode(ToolChoiceMode.AUTO)
        },
        text = ResponseTextConfig(responsesTextMode(request.output.textMode)),
        reasoning = deployment.reasoning,
        metadata = request.traceMetadata.toMap(),
    )
    validateProtocol { mapped.validate() }
    return mapped
}

/** Maps one neutral request into the portable Messages protocol shape. */
public fun mapMessagesRequest(deployment: MessagesDeployment, request: ModelRequest): CreateMessageRequest {
    admit(deployment.targetId, deployment.capabilities, request)
    if (request.traceMetadata.isNotEmpty()) fail(CompatibleProviderError.UNSUPPORTED_METADATA)
    if (request.output.reasoningVisibility && deployment.thinking == null) {
        fail(CompatibleProviderError.UNSUPPORTED_CAPABILITY, capability = ModelCapability.REASONING)
    }
    val maxTokens = request.output.maxOutputTokens ?: deployment.defaultMaxOutputTokens
        ?: fail(CompatibleProviderError.MISSING_OUTPUT_LIMIT)
    val system = mutableListOf<MessagesContentBlock.Text>()
    val turns = mutableListOf<Message>()
    var conversationStarted = false
    request.inputItems.forEach { item ->
        when (item) {
            is ModelInputItem.Message -> when (item.role) {
                ModelRole.SYSTEM, ModelRole.DEVELOPER -> {
                    if (conversationStarted) fail(CompatibleProviderError.UNSUPPORTED_INPUT)
                    item.content.forEach { content ->
                        val text = content as? ModelInputContent.Text
                            ?: fail(CompatibleProviderError.UNSUPPORTED_INPUT)
                        system += MessagesContentBlock.Text(text.text)
                    }
                }
                ModelRole.USER, ModelRole.ASSISTANT -> {
                    conversationStarted = true
                    turns += Message(
                        if (item.role == ModelRole.USER) MessagesRole.USER else MessagesRole.ASSISTANT,
                        MessageContent.Blocks(item.content.map { messagesContent(deployment, it) }),
                    )
                }
            }
            is ModelInputItem.ToolObservation -> {
                conversationStarted = true
                appendMessageBlock(
                    turns,
                    MessagesRole.USER,
                    MessagesContentBlock.ToolResult(
                        toolUseId = item.modelCallId,
                        content = ToolResultContent.Text(item.resultJson),
                    ),
                )
            }
            is ModelInputItem.ToolIntent -> {
                conversationStarted = true
                appendMessageBlock(
                    turns,
                    MessagesRole.ASSISTANT,
                    MessagesContentBlock.ToolUse(
                        id = item.modelCallId,
                        name = wireToolName(item.toolName),
                        input = jsonObject(item.argumentsJson),
                    ),
                )
            }
            is ModelInputItem.ReasoningReference -> fail(CompatibleProviderError.UNSUPPORTED_INPUT)
        }
    }
    val mapped = CreateMessageRequest(
        model = deployment.modelId,
        maxTokens = maxTokens,
        messages = turns,
        stream = ModelCapability.STREAMING in request.requiredCapabilities,
        system = system.takeIf(List<MessagesContentBlock.Text>::isNotEmpty)?.let(SystemPrompt::Blocks),
        tools = request.tools.map(::messagesTool),
        toolChoice = request.tools.takeIf(List<ToolDescriptor>::isNotEmpty)?.let { MessagesToolChoice.Auto() },
        outputConfig = messagesOutput(request.output.textMode),
        thinking = deployment.thinking,
    )
    validateProtocol { mapped.validate() }
    return mapped
}

private fun appendMessageBlock(
    turns: MutableList<Message>,
    role: MessagesRole,
    block: MessagesContentBlock,
) {
    val previous = turns.lastOrNull()
    val blocks = previous?.content as? MessageContent.Blocks
    val mayJoin = block !is MessagesContentBlock.ToolResult ||
        blocks?.value?.all { it is MessagesContentBlock.ToolResult } == true
    if (previous?.role == role && blocks != null && mayJoin) {
        turns[turns.lastIndex] = previous.copy(content = MessageContent.Blocks(blocks.value + block))
    } else {
        turns += Message(role, MessageContent.Blocks(listOf(block)))
    }
}

private fun admit(targetId: String, capabilities: Set<ModelCapability>, request: ModelRequest): Unit {
    request.validate()?.let {
        throw CompatibleProviderException(CompatibleProviderError.INVALID_REQUEST, requestValidation = it)
    }
    if (request.targetId.value != targetId) fail(CompatibleProviderError.TARGET_MISMATCH)
    request.requiredCapabilities.firstOrNull { it !in capabilities }?.let {
        fail(CompatibleProviderError.UNSUPPORTED_CAPABILITY, capability = it)
    }
}

private fun responsesContent(deployment: ResponsesDeployment, content: ModelInputContent): ResponsesContent =
    when (content) {
        is ModelInputContent.Text -> ResponsesContent.Text(content.text)
        is ModelInputContent.MediaReference -> {
            if (content.mediaKind != MediaKind.Image) fail(CompatibleProviderError.UNSUPPORTED_INPUT)
            when (val binding = deployment.mediaBindings[content.reference]
                ?: fail(CompatibleProviderError.MISSING_MEDIA_BINDING)) {
                is ResponsesMediaBinding.Url -> ResponsesContent.Image(imageUrl = binding.value, detail = binding.detail)
                is ResponsesMediaBinding.FileId -> ResponsesContent.Image(fileId = binding.value, detail = binding.detail)
            }
        }
    }

private fun messagesContent(deployment: MessagesDeployment, content: ModelInputContent): MessagesContentBlock =
    when (content) {
        is ModelInputContent.Text -> MessagesContentBlock.Text(content.text)
        is ModelInputContent.MediaReference -> when (val binding = deployment.mediaBindings[content.reference]
            ?: fail(CompatibleProviderError.MISSING_MEDIA_BINDING)) {
            is MessagesMediaBinding.Image -> MessagesContentBlock.Image(binding.source)
            is MessagesMediaBinding.Document -> MessagesContentBlock.Document(binding.source)
        }
    }

private fun responsesTool(tool: ToolDescriptor): FunctionTool = FunctionTool(
    name = wireToolName(tool.name),
    description = tool.description,
    parameters = jsonObject(tool.inputSchemaJson),
    strict = tool.strict,
)

private fun messagesTool(tool: ToolDescriptor): MessagesTool = MessagesTool(
    name = wireToolName(tool.name),
    description = tool.description,
    inputSchema = jsonObject(tool.inputSchemaJson),
    strict = tool.strict,
)

private fun responsesTextMode(mode: TextMode): TextFormat = when (mode) {
    TextMode.Plain -> TextFormat.Text
    TextMode.JsonObject -> TextFormat.JsonObjectFormat
    is TextMode.JsonSchema -> TextFormat.JsonSchema(
        name = "garive_output",
        schema = jsonObject(mode.schemaJson),
        strict = true,
    )
}

private fun messagesOutput(mode: TextMode): OutputConfig? = when (mode) {
    TextMode.Plain -> null
    TextMode.JsonObject -> OutputConfig(format = JsonOutputFormat(JsonObject(mapOf("type" to JsonPrimitive("object")))))
    is TextMode.JsonSchema -> OutputConfig(format = JsonOutputFormat(jsonObject(mode.schemaJson)))
}

private fun jsonObject(encoded: String): JsonObject = try {
    Json.parseToJsonElement(encoded) as? JsonObject ?: fail(CompatibleProviderError.INVALID_JSON_OBJECT)
} catch (error: CompatibleProviderException) {
    throw error
} catch (_: IllegalArgumentException) {
    fail(CompatibleProviderError.INVALID_JSON_OBJECT)
}

private inline fun validateProtocol(block: () -> Unit): Unit = try {
    block()
} catch (_: IllegalArgumentException) {
    fail(CompatibleProviderError.INVALID_PROTOCOL_REQUEST)
}

private fun fail(
    error: CompatibleProviderError,
    capability: ModelCapability? = null,
): Nothing = throw CompatibleProviderException(error, capability = capability)

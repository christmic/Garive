package com.garive.eng.kt.openai

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

/** Official string or item-array input union. */
public sealed interface ResponseInput {
    /** String shorthand input. */
    public data class Text(public val value: String) : ResponseInput
    /** Ordered typed input items. */
    public data class Items(public val value: List<InputItem>) : ResponseInput
}

/** Portable input item union. */
public sealed interface InputItem {
    /** Role-bearing message. */
    public data class Message(public val role: MessageRole, public val content: List<InputContent>) : InputItem
    /** Result for a prior client function call. */
    public data class FunctionCallOutput(
        public val callId: String,
        public val output: FunctionOutput,
        public val status: ItemStatus? = null,
    ) : InputItem
}

/** Portable Responses message role. */
public enum class MessageRole { SYSTEM, DEVELOPER, USER, ASSISTANT }

/** Portable input content block. */
public sealed interface InputContent {
    /** Text supplied to the model. */
    public data class Text(public val text: String) : InputContent
    /** Image supplied by exactly one opaque reference. */
    public data class Image(
        public val imageUrl: String? = null,
        public val fileId: String? = null,
        public val detail: ImageDetail? = null,
    ) : InputContent
}

/** Image fidelity hint. */
public enum class ImageDetail { AUTO, LOW, HIGH }

/** Function-result string shorthand or ordered result content. */
public sealed interface FunctionOutput {
    /** String shorthand. */
    public data class Text(public val value: String) : FunctionOutput
    /** Ordered text/image result blocks. */
    public data class Content(public val value: List<InputContent>) : FunctionOutput
}

/** Item lifecycle status. */
public enum class ItemStatus { IN_PROGRESS, COMPLETED, INCOMPLETE }

/** Client-executed function tool. */
public data class FunctionTool(
    public val name: String,
    public val description: String? = null,
    public val parameters: JsonObject,
    public val strict: Boolean,
)

/** Portable tool selection. */
public sealed interface ToolChoice {
    /** String mode. */
    public data class Mode(public val value: ToolChoiceMode) : ToolChoice
    /** One named client function. */
    public data class Function(public val name: String) : ToolChoice
}

/** Portable tool-choice modes. */
public enum class ToolChoiceMode { NONE, AUTO, REQUIRED }

/** Text response configuration. */
public data class ResponseTextConfig(public val format: TextFormat)

/** Portable output text format. */
public sealed interface TextFormat {
    /** Ordinary text output. */
    public data object Text : TextFormat
    /** Unnamed JSON object mode. */
    public data object JsonObjectFormat : TextFormat
    /** Named strict JSON Schema mode. */
    public data class JsonSchema(
        public val name: String,
        public val description: String? = null,
        public val schema: JsonObject,
        public val strict: Boolean,
    ) : TextFormat
}

/** Optional reasoning controls from the standard create shape. */
public data class ReasoningConfig(public val effort: ReasoningEffort? = null, public val summary: ReasoningSummary? = null)

/** Official portable reasoning effort values. */
public enum class ReasoningEffort { NONE, MINIMAL, LOW, MEDIUM, HIGH, XHIGH, MAX }

/** Official reasoning summary modes. */
public enum class ReasoningSummary { AUTO, CONCISE, DETAILED }

/** Context truncation behavior. */
public enum class Truncation { DISABLED, AUTO }

/** Optional core streaming controls. */
public data class StreamOptions(public val includeObfuscation: Boolean? = null)

/** Typed portable create request; hosted fields live only in [extensions]. */
public data class CreateResponseRequest(
    public val model: String,
    public val input: ResponseInput,
    public val stream: Boolean,
    public val maxOutputTokens: ULong? = null,
    public val temperature: Double? = null,
    public val topP: Double? = null,
    public val truncation: Truncation? = null,
    public val tools: List<FunctionTool> = emptyList(),
    public val toolChoice: ToolChoice? = null,
    public val parallelToolCalls: Boolean? = null,
    public val text: ResponseTextConfig? = null,
    public val reasoning: ReasoningConfig? = null,
    public val metadata: Map<String, String> = emptyMap(),
    public val streamOptions: StreamOptions? = null,
    public val extensions: JsonObject = JsonObject(emptyMap()),
) {
    /** Validates the official portable profile before encoding. */
    public fun validate(): Unit {
        require(model.isNotEmpty())
        when (input) {
            is ResponseInput.Text -> require(input.value.isNotEmpty())
            is ResponseInput.Items -> {
                require(input.value.isNotEmpty())
                input.value.forEach(::validateItem)
            }
        }
        require(temperature == null || temperature.isFinite() && temperature in 0.0..2.0)
        require(topP == null || topP.isFinite() && topP in 0.0..1.0)
        require(metadata.size <= 16 && metadata.all { (key, value) -> key.isNotEmpty() && key.length <= 64 && value.length <= 512 })
        require(tools.map(FunctionTool::name).distinct().size == tools.size)
        tools.forEach { require(it.name.isNotEmpty()) }
        if (toolChoice is ToolChoice.Function) require(toolChoice.name.isNotEmpty())
        if (text?.format is TextFormat.JsonSchema) require(text.format.name.isNotEmpty())
        require(extensions.keys.none { it in TYPED_REQUEST_FIELDS })
    }
}

internal fun CreateResponseRequest.toJson(): JsonObject {
    validate()
    return buildJsonObject {
        put("model", model)
        put("input", when (val input = input) {
            is ResponseInput.Text -> JsonPrimitive(input.value)
            is ResponseInput.Items -> JsonArray(input.value.map(InputItem::toJson))
        })
        put("stream", stream)
        maxOutputTokens?.let { require(it <= Long.MAX_VALUE.toULong()); put("max_output_tokens", it.toLong()) }
        temperature?.let { put("temperature", it) }; topP?.let { put("top_p", it) }
        truncation?.let { put("truncation", it.wire()) }
        if (tools.isNotEmpty()) put("tools", JsonArray(tools.map(FunctionTool::toJson)))
        toolChoice?.let { put("tool_choice", it.toJson()) }
        parallelToolCalls?.let { put("parallel_tool_calls", it) }
        text?.let { put("text", it.toJson()) }; reasoning?.let { put("reasoning", it.toJson()) }
        if (metadata.isNotEmpty()) put("metadata", JsonObject(metadata.mapValues { JsonPrimitive(it.value) }))
        streamOptions?.let { put("stream_options", it.toJson()) }
        extensions.forEach(::put)
    }
}

private fun validateItem(item: InputItem): Unit {
    when (item) {
        is InputItem.Message -> {
            require(item.content.isNotEmpty())
            item.content.forEach(::validateContent)
        }
        is InputItem.FunctionCallOutput -> {
            require(item.callId.isNotEmpty())
            if (item.output is FunctionOutput.Content) {
                require(item.output.value.isNotEmpty())
                item.output.value.forEach(::validateContent)
            }
        }
    }
}

private fun validateContent(content: InputContent): Unit = when (content) {
    is InputContent.Text -> require(content.text.isNotEmpty())
    is InputContent.Image -> {
        require((content.imageUrl == null) != (content.fileId == null))
        require(!content.imageUrl.orEmpty().isEmpty() || !content.fileId.orEmpty().isEmpty())
    }
}

private fun InputItem.toJson(): JsonObject = buildJsonObject {
    when (this@toJson) {
        is InputItem.Message -> {
            put("type", "message"); put("role", role.wire())
            put("content", JsonArray(content.map(InputContent::toJson)))
        }
        is InputItem.FunctionCallOutput -> {
            put("type", "function_call_output"); put("call_id", callId)
            put("output", when (val output = output) {
                is FunctionOutput.Text -> JsonPrimitive(output.value)
                is FunctionOutput.Content -> JsonArray(output.value.map(InputContent::toJson))
            })
            status?.let { put("status", it.wire()) }
        }
    }
}

private fun InputContent.toJson(): JsonObject = buildJsonObject {
    when (this@toJson) {
        is InputContent.Text -> { put("type", "input_text"); put("text", text) }
        is InputContent.Image -> {
            put("type", "input_image")
            imageUrl?.let { put("image_url", it) }; fileId?.let { put("file_id", it) }
            detail?.let { put("detail", it.wire()) }
        }
    }
}

private fun FunctionTool.toJson(): JsonObject = buildJsonObject {
    put("type", "function"); put("name", name); description?.let { put("description", it) }
    put("parameters", parameters); put("strict", strict)
}

private fun ToolChoice.toJson(): JsonElement = when (this) {
    is ToolChoice.Mode -> JsonPrimitive(value.wire())
    is ToolChoice.Function -> buildJsonObject { put("type", "function"); put("name", name) }
}

private fun ResponseTextConfig.toJson(): JsonObject = buildJsonObject {
    put("format", when (val format = format) {
        TextFormat.Text -> buildJsonObject { put("type", "text") }
        TextFormat.JsonObjectFormat -> buildJsonObject { put("type", "json_object") }
        is TextFormat.JsonSchema -> buildJsonObject {
            put("type", "json_schema"); put("name", format.name)
            format.description?.let { put("description", it) }; put("schema", format.schema); put("strict", format.strict)
        }
    })
}

private fun ReasoningConfig.toJson(): JsonObject = buildJsonObject {
    effort?.let { put("effort", it.wire()) }; summary?.let { put("summary", it.wire()) }
}
private fun StreamOptions.toJson(): JsonObject = buildJsonObject { includeObfuscation?.let { put("include_obfuscation", it) } }
private fun Enum<*>.wire(): String = name.lowercase()

private val TYPED_REQUEST_FIELDS: Set<String> = setOf(
    "model", "input", "stream", "max_output_tokens", "temperature", "top_p", "truncation",
    "tools", "tool_choice", "parallel_tool_calls", "text", "reasoning", "metadata", "stream_options",
)

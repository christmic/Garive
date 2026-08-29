package com.garive.eng.kt.anthropic

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

/** Official string or block-array message content union. */
public sealed interface MessageContent {
    /** String shorthand content. */
    public data class Text(public val value: String) : MessageContent
    /** Ordered portable content blocks. */
    public data class Blocks(public val value: List<ContentBlock>) : MessageContent
}

/** One user or assistant turn. */
public data class Message(public val role: MessageRole, public val content: MessageContent)

/** Official input message roles. */
public enum class MessageRole { USER, ASSISTANT }

/** Official ephemeral prompt-cache marker. */
public data class CacheControl(public val ttl: CacheTtl? = null)

/** Prompt-cache time-to-live values. */
public enum class CacheTtl(public val wireName: String) { FIVE_MINUTES("5m"), ONE_HOUR("1h") }

/** Portable request content block. */
public sealed interface ContentBlock {
    /** Text block. */
    public data class Text(
        public val text: String,
        public val cacheControl: CacheControl? = null,
        public val citations: List<JsonObject>? = null,
    ) : ContentBlock
    /** Image block. */
    public data class Image(public val source: ImageSource, public val cacheControl: CacheControl? = null) : ContentBlock
    /** Document block. */
    public data class Document(
        public val source: DocumentSource,
        public val cacheControl: CacheControl? = null,
        public val citations: CitationsConfig? = null,
        public val title: String? = null,
        public val context: String? = null,
    ) : ContentBlock
    /** Prior client tool invocation. */
    public data class ToolUse(
        public val id: String,
        public val name: String,
        public val input: JsonObject,
        public val cacheControl: CacheControl? = null,
    ) : ContentBlock
    /** Result for a prior client tool invocation. */
    public data class ToolResult(
        public val toolUseId: String,
        public val content: ToolResultContent? = null,
        public val isError: Boolean? = null,
        public val cacheControl: CacheControl? = null,
    ) : ContentBlock
    /** Prior extended thinking. */
    public data class Thinking(public val thinking: String, public val signature: String) : ContentBlock
    /** Opaque prior redacted thinking. */
    public data class RedactedThinking(public val data: String) : ContentBlock
}

/** Official image source union. */
public sealed interface ImageSource {
    /** Base64 image. */
    public data class Base64(public val mediaType: ImageMediaType, public val data: String) : ImageSource
    /** URL image. */
    public data class Url(public val url: String) : ImageSource
}

/** Official image media types. */
public enum class ImageMediaType(public val wireName: String) {
    JPEG("image/jpeg"), PNG("image/png"), GIF("image/gif"), WEBP("image/webp"),
}

/** Official document source union. */
public sealed interface DocumentSource {
    /** Base64 PDF. */
    public data class Base64(public val data: String) : DocumentSource
    /** URL PDF. */
    public data class Url(public val url: String) : DocumentSource
    /** Plain text document. */
    public data class Text(public val data: String) : DocumentSource
    /** String or nested text/image content. */
    public data class Content(public val content: DocumentContent) : DocumentSource
}

/** Nested document content union. */
public sealed interface DocumentContent {
    /** String shorthand. */
    public data class Text(public val value: String) : DocumentContent
    /** Ordered text/image blocks. */
    public data class Blocks(public val value: List<DocumentContentBlock>) : DocumentContent
}

/** Nested document source block. */
public sealed interface DocumentContentBlock {
    /** Nested text. */
    public data class Text(public val text: String, public val cacheControl: CacheControl? = null) : DocumentContentBlock
    /** Nested image. */
    public data class Image(public val source: ImageSource, public val cacheControl: CacheControl? = null) : DocumentContentBlock
}

/** Tool-result string shorthand or portable blocks. */
public sealed interface ToolResultContent {
    /** String shorthand. */
    public data class Text(public val value: String) : ToolResultContent
    /** Ordered text/image/document blocks. */
    public data class Blocks(public val value: List<ContentBlock>) : ToolResultContent
}

/** Top-level system prompt. */
public sealed interface SystemPrompt {
    /** String shorthand. */
    public data class Text(public val value: String) : SystemPrompt
    /** Ordered text blocks. */
    public data class Blocks(public val value: List<ContentBlock.Text>) : SystemPrompt
}

/** Citation generation setting. */
public data class CitationsConfig(public val enabled: Boolean? = null)

/** Client-executed tool definition. */
public data class Tool(
    public val name: String,
    public val inputSchema: JsonObject,
    public val description: String? = null,
    public val strict: Boolean? = null,
    public val cacheControl: CacheControl? = null,
)

/** Portable tool choice. */
public sealed interface ToolChoice {
    public data class Auto(public val disableParallelToolUse: Boolean? = null) : ToolChoice
    public data class Any(public val disableParallelToolUse: Boolean? = null) : ToolChoice
    public data class Tool(public val name: String, public val disableParallelToolUse: Boolean? = null) : ToolChoice
    public data object None : ToolChoice
}

/** Official output effort levels. */
public enum class Effort { LOW, MEDIUM, HIGH, XHIGH, MAX }

/** Official JSON Schema output format. */
public data class JsonOutputFormat(public val schema: JsonObject)

/** Output format and effort controls. */
public data class OutputConfig(public val effort: Effort? = null, public val format: JsonOutputFormat? = null)

/** Extended-thinking display policy. */
public enum class ThinkingDisplay { SUMMARIZED, OMITTED }

/** Extended-thinking configuration. */
public sealed interface ThinkingConfig {
    public data object Disabled : ThinkingConfig
    public data class Enabled(public val budgetTokens: ULong, public val display: ThinkingDisplay? = null) : ThinkingConfig
    public data class Adaptive(public val display: ThinkingDisplay? = null) : ThinkingConfig
}

/** Protocol request metadata. */
public data class Metadata(public val userId: String? = null)

/** Typed portable create request; hosted fields remain [extensions]. */
public data class CreateMessageRequest(
    public val model: String,
    public val maxTokens: ULong,
    public val messages: List<Message>,
    public val stream: Boolean,
    public val system: SystemPrompt? = null,
    public val stopSequences: List<String> = emptyList(),
    public val temperature: Double? = null,
    public val topP: Double? = null,
    public val topK: ULong? = null,
    public val tools: List<Tool> = emptyList(),
    public val toolChoice: ToolChoice? = null,
    public val outputConfig: OutputConfig? = null,
    public val thinking: ThinkingConfig? = null,
    public val metadata: Metadata? = null,
    public val extensions: JsonObject = JsonObject(emptyMap()),
) {
    /** Validates the official portable profile before encoding. */
    public fun validate(): Unit {
        require(model.isNotEmpty() && messages.isNotEmpty())
        messages.forEach { require(it.content.valid()) }
        require(stopSequences.none(String::isEmpty))
        require(temperature == null || temperature.isFinite() && temperature in 0.0..1.0)
        require(topP == null || topP.isFinite() && topP in 0.0..1.0)
        system?.let { prompt ->
            require(when (prompt) {
                is SystemPrompt.Text -> prompt.value.isNotEmpty()
                is SystemPrompt.Blocks -> prompt.value.isNotEmpty() && prompt.value.all { it.valid() }
            })
        }
        require(tools.map(Tool::name).distinct().size == tools.size && tools.all { it.name.isNotEmpty() })
        if (toolChoice is ToolChoice.Tool) require(toolChoice.name.isNotEmpty())
        if (thinking is ThinkingConfig.Enabled) require(thinking.budgetTokens >= 1_024u && thinking.budgetTokens < maxTokens)
        require(extensions.keys.none { it in MessageFields.CREATE })
    }
}

internal fun CreateMessageRequest.toJson(): JsonObject {
    validate()
    return buildJsonObject {
        put("model", model); require(maxTokens <= Long.MAX_VALUE.toULong()); put("max_tokens", maxTokens.toLong())
        put("messages", JsonArray(messages.map(Message::toJson))); put("stream", stream)
        system?.let { put("system", it.toJson()) }
        if (stopSequences.isNotEmpty()) put("stop_sequences", JsonArray(stopSequences.map(::JsonPrimitive)))
        temperature?.let { put("temperature", it) }; topP?.let { put("top_p", it) }
        topK?.let { require(it <= Long.MAX_VALUE.toULong()); put("top_k", it.toLong()) }
        if (tools.isNotEmpty()) put("tools", JsonArray(tools.map(Tool::toJson)))
        toolChoice?.let { put("tool_choice", it.toJson()) }; outputConfig?.let { put("output_config", it.toJson()) }
        thinking?.let { put("thinking", it.toJson()) }; metadata?.let { put("metadata", it.toJson()) }
        extensions.forEach(::put)
    }
}

/** Validates and encodes one portable create request as protocol JSON. */
public fun encodeCreateMessageRequest(request: CreateMessageRequest): JsonObject = request.toJson()

private fun Message.toJson(): JsonObject = buildJsonObject {
    put("role", role.wire()); put("content", content.toJson())
}

private fun MessageContent.valid(): Boolean = when (this) {
    is MessageContent.Text -> value.isNotEmpty()
    is MessageContent.Blocks -> value.isNotEmpty() && value.all(ContentBlock::valid)
}

private fun MessageContent.toJson(): JsonElement = when (this) {
    is MessageContent.Text -> JsonPrimitive(value)
    is MessageContent.Blocks -> JsonArray(value.map(ContentBlock::toJson))
}

private fun ContentBlock.valid(): Boolean = when (this) {
    is ContentBlock.Text -> text.isNotEmpty()
    is ContentBlock.Image -> source.valid()
    is ContentBlock.Document -> source.valid()
    is ContentBlock.ToolUse -> id.isNotEmpty() && name.isNotEmpty()
    is ContentBlock.ToolResult -> toolUseId.isNotEmpty() && when (val result = content) {
        null, is ToolResultContent.Text -> true
        is ToolResultContent.Blocks -> result.value.isNotEmpty() && result.value.all {
            it is ContentBlock.Text || it is ContentBlock.Image || it is ContentBlock.Document
        } && result.value.all(ContentBlock::valid)
    }
    is ContentBlock.Thinking -> thinking.isNotEmpty() && signature.isNotEmpty()
    is ContentBlock.RedactedThinking -> data.isNotEmpty()
}

private fun ContentBlock.toJson(): JsonObject = buildJsonObject {
    when (val block = this@toJson) {
        is ContentBlock.Text -> { put(MessageFields.TYPE, MessageKinds.TEXT); put("text", block.text); block.cacheControl?.let { put("cache_control", it.toJson()) }; block.citations?.let { put("citations", JsonArray(it)) } }
        is ContentBlock.Image -> { put(MessageFields.TYPE, MessageKinds.IMAGE); put("source", block.source.toJson()); block.cacheControl?.let { put("cache_control", it.toJson()) } }
        is ContentBlock.Document -> { put(MessageFields.TYPE, MessageKinds.DOCUMENT); put("source", block.source.toJson()); block.cacheControl?.let { put("cache_control", it.toJson()) }; block.citations?.let { put("citations", it.toJson()) }; block.title?.let { put("title", it) }; block.context?.let { put("context", it) } }
        is ContentBlock.ToolUse -> { put(MessageFields.TYPE, MessageKinds.TOOL_USE); put("id", block.id); put("name", block.name); put("input", block.input); block.cacheControl?.let { put("cache_control", it.toJson()) } }
        is ContentBlock.ToolResult -> { put(MessageFields.TYPE, MessageKinds.TOOL_RESULT); put("tool_use_id", block.toolUseId); block.content?.let { put("content", it.toJson()) }; block.isError?.let { put("is_error", it) }; block.cacheControl?.let { put("cache_control", it.toJson()) } }
        is ContentBlock.Thinking -> { put(MessageFields.TYPE, MessageKinds.THINKING); put("thinking", block.thinking); put("signature", block.signature) }
        is ContentBlock.RedactedThinking -> { put(MessageFields.TYPE, MessageKinds.REDACTED_THINKING); put("data", block.data) }
    }
}

private fun ImageSource.valid(): Boolean = when (this) { is ImageSource.Base64 -> data.isNotEmpty(); is ImageSource.Url -> url.isNotEmpty() }
private fun ImageSource.toJson(): JsonObject = buildJsonObject { when (val source = this@toJson) { is ImageSource.Base64 -> { put(MessageFields.TYPE, MessageKinds.BASE64); put("media_type", source.mediaType.wireName); put("data", source.data) }; is ImageSource.Url -> { put(MessageFields.TYPE, MessageKinds.URL); put("url", source.url) } } }
private fun DocumentSource.valid(): Boolean = when (this) { is DocumentSource.Base64 -> data.isNotEmpty(); is DocumentSource.Url -> url.isNotEmpty(); is DocumentSource.Text -> data.isNotEmpty(); is DocumentSource.Content -> content.valid() }
private fun DocumentSource.toJson(): JsonObject = buildJsonObject { when (val source = this@toJson) { is DocumentSource.Base64 -> { put(MessageFields.TYPE, MessageKinds.BASE64); put("media_type", HttpWire.MEDIA_PDF); put("data", source.data) }; is DocumentSource.Url -> { put(MessageFields.TYPE, MessageKinds.URL); put("url", source.url) }; is DocumentSource.Text -> { put(MessageFields.TYPE, MessageKinds.TEXT); put("media_type", HttpWire.MEDIA_TEXT); put("data", source.data) }; is DocumentSource.Content -> { put(MessageFields.TYPE, MessageKinds.CONTENT); put("content", source.content.toJson()) } } }
private fun DocumentContent.toJson(): JsonElement = when (this) { is DocumentContent.Text -> JsonPrimitive(value); is DocumentContent.Blocks -> JsonArray(value.map { block -> when (block) { is DocumentContentBlock.Text -> ContentBlock.Text(block.text, block.cacheControl).toJson(); is DocumentContentBlock.Image -> ContentBlock.Image(block.source, block.cacheControl).toJson() } }) }
private fun DocumentContent.valid(): Boolean = when (this) { is DocumentContent.Text -> value.isNotEmpty(); is DocumentContent.Blocks -> value.isNotEmpty() && value.all { block -> when (block) { is DocumentContentBlock.Text -> block.text.isNotEmpty(); is DocumentContentBlock.Image -> block.source.valid() } } }
private fun ToolResultContent.toJson(): JsonElement = when (this) { is ToolResultContent.Text -> JsonPrimitive(value); is ToolResultContent.Blocks -> JsonArray(value.map(ContentBlock::toJson)) }
private fun SystemPrompt.toJson(): JsonElement = when (this) { is SystemPrompt.Text -> JsonPrimitive(value); is SystemPrompt.Blocks -> JsonArray(value.map(ContentBlock.Text::toJson)) }
private fun CacheControl.toJson(): JsonObject = buildJsonObject { put(MessageFields.TYPE, MessageKinds.EPHEMERAL); ttl?.let { put("ttl", it.wireName) } }
private fun CitationsConfig.toJson(): JsonObject = buildJsonObject { enabled?.let { put("enabled", it) } }
private fun Tool.toJson(): JsonObject = buildJsonObject { put("name", name); put("input_schema", inputSchema); description?.let { put("description", it) }; strict?.let { put("strict", it) }; cacheControl?.let { put("cache_control", it.toJson()) } }
private fun ToolChoice.toJson(): JsonObject = buildJsonObject { when (val choice = this@toJson) { is ToolChoice.Auto -> { put(MessageFields.TYPE, MessageKinds.AUTO); choice.disableParallelToolUse?.let { put("disable_parallel_tool_use", it) } }; is ToolChoice.Any -> { put(MessageFields.TYPE, MessageKinds.ANY); choice.disableParallelToolUse?.let { put("disable_parallel_tool_use", it) } }; is ToolChoice.Tool -> { put(MessageFields.TYPE, MessageKinds.TOOL); put("name", choice.name); choice.disableParallelToolUse?.let { put("disable_parallel_tool_use", it) } }; ToolChoice.None -> put(MessageFields.TYPE, MessageKinds.NONE) } }
private fun OutputConfig.toJson(): JsonObject = buildJsonObject { effort?.let { put("effort", it.wire()) }; format?.let { put("format", buildJsonObject { put(MessageFields.TYPE, MessageKinds.JSON_SCHEMA); put("schema", it.schema) }) } }
private fun ThinkingConfig.toJson(): JsonObject = buildJsonObject { when (val value = this@toJson) { ThinkingConfig.Disabled -> put(MessageFields.TYPE, MessageKinds.DISABLED); is ThinkingConfig.Enabled -> { put(MessageFields.TYPE, MessageKinds.ENABLED); put("budget_tokens", value.budgetTokens.toLong()); value.display?.let { put("display", it.wire()) } }; is ThinkingConfig.Adaptive -> { put(MessageFields.TYPE, MessageKinds.ADAPTIVE); value.display?.let { put("display", it.wire()) } } } }
private fun Metadata.toJson(): JsonObject = buildJsonObject { userId?.let { put("user_id", it) } }
private fun Enum<*>.wire(): String = name.lowercase()

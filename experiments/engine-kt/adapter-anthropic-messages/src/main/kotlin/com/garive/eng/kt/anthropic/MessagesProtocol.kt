package com.garive.eng.kt.anthropic

import java.net.URI
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put

/** Caller-supplied protocol header with explicit diagnostic redaction. */
public class ProtocolHeader private constructor(
    public val name: String,
    public val value: String,
    public val sensitive: Boolean,
) {
    public companion object {
        /** Validates and creates a header. */
        public fun create(name: String, value: String, sensitive: Boolean): ProtocolHeader {
            require(name.matches(Regex("[!#$%&'*+.^_`|~0-9A-Za-z-]+")))
            require(value.none { it == '\r' || it == '\n' || it == '\u0000' })
            return ProtocolHeader(name.lowercase(), value, sensitive)
        }
    }

    override fun toString(): String = "ProtocolHeader(name=$name, value=${if (sensitive) "<redacted>" else value}, sensitive=$sensitive)"
    override fun equals(other: Any?): Boolean = other is ProtocolHeader && name == other.name && value == other.value && sensitive == other.sensitive
    override fun hashCode(): Int = 31 * (31 * name.hashCode() + value.hashCode()) + sensitive.hashCode()
}

/** Explicit Messages-compatible deployment configuration. */
public data class MessagesAdapterConfig(
    public val endpoint: String,
    public val headers: List<ProtocolHeader>,
    public val versionHeaderName: String,
    public val protocolVersion: String,
) {
    init {
        val uri = runCatching { URI(endpoint) }.getOrNull()
        require(uri != null && uri.isAbsolute && uri.host != null && uri.scheme in setOf("http", "https"))
        require(protocolVersion.isNotEmpty())
        val version = ProtocolHeader.create(versionHeaderName, protocolVersion, false)
        require(version.name !in setOf("content-type", "accept"))
        require(headers.map { it.name }.distinct().size == headers.size)
        require(headers.none { it.name in setOf("content-type", "accept", version.name) })
    }
}

/** One complete request for a Runtime-owned transport. */
public class ProtocolHttpRequest internal constructor(
    public val uri: String,
    public val headers: List<ProtocolHeader>,
    public val body: ByteArray,
) {
    /** Messages-compatible requests use POST. */
    public val method: String = "POST"
}

/** Official string or block-array message content union. */
public sealed interface MessageContent {
    /** String shorthand content. */
    public data class Text(public val value: String) : MessageContent
    /** Ordered official content-block objects. */
    public data class Blocks(public val value: List<JsonObject>) : MessageContent
}

/** One user or assistant turn. */
public data class Message(public val role: String, public val content: MessageContent) {
    init { require(role in setOf("user", "assistant")) }

    internal fun json(): JsonObject = buildJsonObject {
        put("role", role)
        put("content", when (val content = content) {
            is MessageContent.Text -> JsonPrimitive(content.value)
            is MessageContent.Blocks -> JsonArray(content.value)
        })
    }
}

/** Typed portable create request; hosted fields remain [extensions]. */
public data class CreateMessageRequest(
    public val model: String,
    public val maxTokens: ULong,
    public val messages: List<Message>,
    public val stream: Boolean,
    public val system: JsonElement? = null,
    public val stopSequences: List<String> = emptyList(),
    public val temperature: Double? = null,
    public val topP: Double? = null,
    public val topK: ULong? = null,
    public val tools: List<JsonObject> = emptyList(),
    public val toolChoice: JsonObject? = null,
    public val outputConfig: JsonObject? = null,
    public val thinking: JsonObject? = null,
    public val metadata: JsonObject? = null,
    public val extensions: JsonObject = JsonObject(emptyMap()),
) {
    /** Validates the official portable profile before encoding. */
    public fun validate(): Unit {
        require(model.isNotEmpty() && messages.isNotEmpty())
        require(messages.all { message -> when (val content = message.content) {
            is MessageContent.Text -> content.value.isNotEmpty()
            is MessageContent.Blocks -> content.value.isNotEmpty()
        } })
        require(stopSequences.none(String::isEmpty))
        require(temperature == null || temperature.isFinite() && temperature in 0.0..1.0)
        require(topP == null || topP.isFinite() && topP in 0.0..1.0)
        require(extensions.keys.none { it in TYPED_FIELDS })
    }
}

/** Protocol-only Messages adapter; it owns no retry or model mapping. */
public class MessagesAdapter(public val config: MessagesAdapterConfig) {
    /** Encodes a validated official create request. */
    public fun prepare(request: CreateMessageRequest): ProtocolHttpRequest {
        request.validate()
        val body = buildJsonObject {
            put("model", request.model)
            require(request.maxTokens <= Long.MAX_VALUE.toULong()); put("max_tokens", request.maxTokens.toLong())
            put("messages", JsonArray(request.messages.map(Message::json)))
            put("stream", request.stream)
            request.system?.let { put("system", it) }
            if (request.stopSequences.isNotEmpty()) put("stop_sequences", JsonArray(request.stopSequences.map(::JsonPrimitive)))
            request.temperature?.let { put("temperature", it) }; request.topP?.let { put("top_p", it) }
            request.topK?.let { require(it <= Long.MAX_VALUE.toULong()); put("top_k", it.toLong()) }
            if (request.tools.isNotEmpty()) put("tools", JsonArray(request.tools))
            request.toolChoice?.let { put("tool_choice", it) }; request.outputConfig?.let { put("output_config", it) }
            request.thinking?.let { put("thinking", it) }; request.metadata?.let { put("metadata", it) }
            request.extensions.forEach(::put)
        }
        val headers = config.headers + listOf(
            ProtocolHeader.create(config.versionHeaderName, config.protocolVersion, false),
            ProtocolHeader.create("content-type", "application/json", false),
            ProtocolHeader.create("accept", if (request.stream) "text/event-stream" else "application/json", false),
        )
        return ProtocolHttpRequest(config.endpoint, headers, body.toString().encodeToByteArray())
    }

    /** Decodes ordinary JSON while retaining status and headers as facts. */
    public fun decodeResponse(status: Int, headers: List<ProtocolHeader>, body: ByteArray): DecodedResponse {
        requireJsonMedia(headers)
        val value = JSON.parseToJsonElement(body.decodeToString()).jsonObject
        return if (status in 200..299) DecodedResponse.Message(status, headers, MessageResponse.parse(value))
        else DecodedResponse.Error(status, headers, ErrorEnvelope.parse(value))
    }
}

/** Portable output content plus lossless hosted/future blocks. */
public sealed interface OutputBlock {
    /** Text output. */
    public data class Text(public val value: JsonObject) : OutputBlock
    /** Extended thinking. */
    public data class Thinking(public val value: JsonObject) : OutputBlock
    /** Opaque redacted thinking. */
    public data class RedactedThinking(public val value: JsonObject) : OutputBlock
    /** Client tool invocation. */
    public data class ToolUse(public val value: JsonObject) : OutputBlock
    /** Hosted or future output without promoted semantics. */
    public data class Extension(public val discriminator: String, public val value: JsonObject) : OutputBlock
}

/** Official ordinary Message response. */
public data class MessageResponse(
    public val id: String,
    public val model: String,
    public val stopReason: String?,
    public val content: List<OutputBlock>,
    public val usage: JsonObject,
    public val raw: JsonObject,
) {
    public companion object {
        /** Parses required fields and portable output variants. */
        public fun parse(value: JsonObject): MessageResponse {
            require(value.text("type") == "message" && value.text("role") == "assistant")
            val content = value.array("content").map { element ->
                val block = element.jsonObject
                when (val type = block.text("type")) {
                    "text" -> OutputBlock.Text(block); "thinking" -> OutputBlock.Thinking(block)
                    "redacted_thinking" -> OutputBlock.RedactedThinking(block); "tool_use" -> OutputBlock.ToolUse(block)
                    else -> OutputBlock.Extension(type, block)
                }
            }
            return MessageResponse(value.text("id"), value.text("model"), value["stop_reason"]?.jsonPrimitive?.contentOrNull, content, value.getValue("usage").jsonObject, value)
        }
    }
}

/** Official error envelope with an open error type. */
public data class ErrorEnvelope(public val type: String, public val message: String, public val requestId: String?, public val raw: JsonObject) {
    public companion object {
        /** Parses the standard outer error object. */
        public fun parse(value: JsonObject): ErrorEnvelope {
            require(value.text("type") == "error")
            val error = value.getValue("error").jsonObject
            return ErrorEnvelope(error.text("type"), error.text("message"), value["request_id"]?.jsonPrimitive?.contentOrNull, value)
        }
    }
}

/** Ordinary HTTP result without provider classification. */
public sealed interface DecodedResponse {
    /** Successful message fact. */
    public data class Message(public val status: Int, public val headers: List<ProtocolHeader>, public val message: MessageResponse) : DecodedResponse
    /** Non-success error fact. */
    public data class Error(public val status: Int, public val headers: List<ProtocolHeader>, public val error: ErrorEnvelope) : DecodedResponse
}

/** Typed event kind; unknown types remain extensions. */
public sealed interface StreamEventKind {
    /** Known portable event discriminator. */
    public data class Portable(public val type: String) : StreamEventKind
    /** Future event discriminator. */
    public data class Extension(public val type: String) : StreamEventKind
}

/** One lossless typed Messages event. */
public data class StreamEvent(public val kind: StreamEventKind, public val raw: JsonObject)

/** Incremental Messages SSE and lifecycle decoder. */
public class MessagesStreamDecoder {
    private var buffer: ByteArray = byteArrayOf()
    private var started: Boolean = false
    private var terminal: Boolean = false
    private var messageDelta: Boolean = false
    private val blocks: MutableMap<UInt, String> = mutableMapOf()
    private val toolJson: MutableMap<UInt, StringBuilder> = mutableMapOf()

    /** Appends arbitrary transport bytes and emits complete validated events. */
    public fun push(bytes: ByteArray): List<StreamEvent> {
        buffer += bytes; val events = mutableListOf<StreamEvent>()
        while (true) {
            val boundary = findBoundary(buffer) ?: break
            val frame = parseFrame(buffer.copyOfRange(0, boundary.first))
            buffer = buffer.copyOfRange(boundary.first + boundary.second, buffer.size)
            if (frame != null) events += accept(frame)
        }
        return events
    }

    /** Requires one terminal and no open blocks at EOF. */
    public fun finish(): Unit {
        val trailing = runCatching { buffer.decodeToString(throwOnInvalidSequence = true) }.getOrNull()
        require(trailing != null && trailing.lineSequence().all { it.isBlank() || it.startsWith(':') })
        require(terminal && blocks.isEmpty()); buffer = byteArrayOf()
    }

    private fun accept(frame: Pair<String?, String>): StreamEvent {
        val raw = JSON.parseToJsonElement(frame.second).jsonObject; val type = raw.text("type")
        require(frame.first == null || frame.first == type); require(!terminal)
        when (type) {
            "ping" -> Unit
            "message_start" -> { require(!started); started = true; require(raw["message"] is JsonObject) }
            "error" -> terminal = true
            else -> { require(started); when (type) {
                "content_block_start" -> { val index = raw.index(); val kind = raw.getValue("content_block").jsonObject.text("type"); require(blocks.put(index, kind) == null); if (kind == "tool_use") toolJson[index] = StringBuilder() }
                "content_block_delta" -> delta(raw)
                "content_block_stop" -> { val index = raw.index(); val kind = requireNotNull(blocks.remove(index)); if (kind == "tool_use") JSON.parseToJsonElement(requireNotNull(toolJson.remove(index)).toString()) }
                "message_delta" -> { require(blocks.isEmpty() && !messageDelta); messageDelta = true }
                "message_stop" -> { require(blocks.isEmpty() && messageDelta); terminal = true }
            } }
        }
        val kind = if (type in PORTABLE_EVENTS) StreamEventKind.Portable(type) else StreamEventKind.Extension(type)
        return StreamEvent(kind, raw)
    }

    private fun delta(raw: JsonObject): Unit {
        val index = raw.index(); val block = requireNotNull(blocks[index]); val delta = raw.getValue("delta").jsonObject
        val type = delta.text("type")
        require(when (block) { "text" -> type in setOf("text_delta", "citations_delta"); "tool_use" -> type == "input_json_delta"; "thinking" -> type in setOf("thinking_delta", "signature_delta"); else -> true })
        if (type == "input_json_delta") requireNotNull(toolJson[index]).append(delta.text("partial_json"))
    }
}

private val JSON: Json = Json { ignoreUnknownKeys = false }
private val TYPED_FIELDS: Set<String> = setOf("model", "max_tokens", "messages", "stream", "system", "stop_sequences", "temperature", "top_p", "top_k", "tools", "tool_choice", "output_config", "thinking", "metadata")
private val PORTABLE_EVENTS: Set<String> = setOf("message_start", "content_block_start", "content_block_delta", "content_block_stop", "message_delta", "message_stop", "ping", "error")
private fun JsonObject.text(name: String): String = getValue(name).jsonPrimitive.content
private fun JsonObject.array(name: String): JsonArray = getValue(name) as JsonArray
private fun JsonObject.index(): UInt = getValue("index").jsonPrimitive.content.toUInt()
private fun requireJsonMedia(headers: List<ProtocolHeader>): Unit {
    require((headers.firstOrNull { it.name == "content-type" }?.value ?: "application/json").substringBefore(';') == "application/json")
}
private fun findBoundary(bytes: ByteArray): Pair<Int, Int>? {
    for (index in bytes.indices) {
        if (index + 1 < bytes.size && bytes[index] == 10.toByte() && bytes[index + 1] == 10.toByte()) return index to 2
        if (index + 3 < bytes.size && bytes[index] == 13.toByte() && bytes[index + 1] == 10.toByte() && bytes[index + 2] == 13.toByte() && bytes[index + 3] == 10.toByte()) return index to 4
    }; return null
}
private fun parseFrame(bytes: ByteArray): Pair<String?, String>? {
    var event: String? = null; val data = mutableListOf<String>()
    bytes.decodeToString(throwOnInvalidSequence = true).lineSequence().forEach { raw ->
        val line = raw.removeSuffix("\r")
        if (line.isNotEmpty() && !line.startsWith(':')) {
            val field = line.substringBefore(':'); val value = line.substringAfter(':', "").removePrefix(" ")
            when (field) { "event" -> event = value; "data" -> data += value; "retry" -> value.toULong(); "id" -> require('\u0000' !in value) }
        }
    }; return if (data.isEmpty()) null else event to data.joinToString("\n")
}

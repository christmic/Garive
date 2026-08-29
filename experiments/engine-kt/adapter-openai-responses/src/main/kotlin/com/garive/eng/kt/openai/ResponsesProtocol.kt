package com.garive.eng.kt.openai

import java.net.URI
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put

/** Stable protocol-only failure; it contains no deployment or retry policy. */
public enum class ResponsesProtocolError {
    INVALID_ENDPOINT, INVALID_HEADER, INVALID_REQUEST, INVALID_JSON, INVALID_MEDIA_TYPE,
    INVALID_SSE, INVALID_LIFECYCLE, TRUNCATED_STREAM,
}

/** Caller-supplied header with an explicit diagnostic redaction marker. */
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

    override fun equals(other: Any?): Boolean =
        other is ProtocolHeader && name == other.name && value == other.value && sensitive == other.sensitive

    override fun hashCode(): Int = 31 * (31 * name.hashCode() + value.hashCode()) + sensitive.hashCode()
}

/** Explicit Responses-compatible endpoint configuration. */
public data class ResponsesAdapterConfig(
    public val endpoint: String,
    public val headers: List<ProtocolHeader>,
) {
    init {
        val uri = runCatching { URI(endpoint) }.getOrNull()
        require(uri != null && uri.isAbsolute && uri.host != null && uri.scheme in setOf("http", "https"))
        require(headers.map { it.name }.distinct().size == headers.size)
        require(headers.none { it.name in setOf("content-type", "accept") })
    }
}

/** One complete request for a Runtime-owned HTTP transport. */
public class ProtocolHttpRequest internal constructor(
    public val uri: String,
    public val headers: List<ProtocolHeader>,
    public val body: ByteArray,
) {
    /** Responses-compatible requests use POST. */
    public val method: String = "POST"
}

/** Official string or item-array input union. */
public sealed interface ResponseInput {
    /** String shorthand input. */
    public data class Text(public val value: String) : ResponseInput
    /** Ordered official input item objects. */
    public data class Items(public val value: List<JsonObject>) : ResponseInput
}

/** Typed portable create request; hosted fields live only in [extensions]. */
public data class CreateResponseRequest(
    public val model: String,
    public val input: ResponseInput,
    public val stream: Boolean,
    public val maxOutputTokens: ULong? = null,
    public val temperature: Double? = null,
    public val topP: Double? = null,
    public val tools: List<JsonObject> = emptyList(),
    public val toolChoice: JsonElement? = null,
    public val text: JsonObject? = null,
    public val reasoning: JsonObject? = null,
    public val metadata: Map<String, String> = emptyMap(),
    public val extensions: JsonObject = JsonObject(emptyMap()),
) {
    /** Validates the official portable profile before encoding. */
    public fun validate(): Unit {
        require(model.isNotEmpty())
        when (input) {
            is ResponseInput.Text -> require(input.value.isNotEmpty())
            is ResponseInput.Items -> require(input.value.isNotEmpty())
        }
        require(temperature == null || temperature.isFinite() && temperature in 0.0..2.0)
        require(topP == null || topP.isFinite() && topP in 0.0..1.0)
        require(metadata.size <= 16)
        require(extensions.keys.none { it in TYPED_REQUEST_FIELDS })
    }
}

/** Protocol-only Responses adapter. It performs exactly one wire exchange. */
public class ResponsesAdapter(public val config: ResponsesAdapterConfig) {
    /** Encodes a validated official create request. */
    public fun prepare(request: CreateResponseRequest): ProtocolHttpRequest {
        request.validate()
        val body = buildJsonObject {
            put("model", request.model)
            put("input", when (val input = request.input) {
                is ResponseInput.Text -> JsonPrimitive(input.value)
                is ResponseInput.Items -> JsonArray(input.value)
            })
            put("stream", request.stream)
            request.maxOutputTokens?.let { require(it <= Long.MAX_VALUE.toULong()); put("max_output_tokens", it.toLong()) }
            request.temperature?.let { put("temperature", it) }
            request.topP?.let { put("top_p", it) }
            if (request.tools.isNotEmpty()) put("tools", JsonArray(request.tools))
            request.toolChoice?.let { put("tool_choice", it) }
            request.text?.let { put("text", it) }
            request.reasoning?.let { put("reasoning", it) }
            if (request.metadata.isNotEmpty()) put("metadata", JsonObject(request.metadata.mapValues { JsonPrimitive(it.value) }))
            request.extensions.forEach(::put)
        }
        val headers = config.headers + listOf(
            ProtocolHeader.create("content-type", "application/json", false),
            ProtocolHeader.create("accept", if (request.stream) "text/event-stream" else "application/json", false),
        )
        return ProtocolHttpRequest(config.endpoint, headers, body.toString().encodeToByteArray())
    }

    /** Decodes ordinary JSON while retaining HTTP status and headers as facts. */
    public fun decodeResponse(status: Int, headers: List<ProtocolHeader>, body: ByteArray): DecodedResponse {
        requireJsonMedia(headers)
        val value = JSON.parseToJsonElement(body.decodeToString()).jsonObject
        return if (status in 200..299) {
            val response = ResponseEnvelope.parse(value)
            DecodedResponse.Response(status, headers, response)
        } else {
            val error = ErrorEnvelope.parse(value)
            DecodedResponse.Error(status, headers, error)
        }
    }
}

/** Typed portable response output item with lossless extensions. */
public sealed interface ResponseOutputItem {
    /** Assistant message output. */
    public data class Message(public val value: JsonObject) : ResponseOutputItem
    /** Client function call. */
    public data class FunctionCall(public val value: JsonObject) : ResponseOutputItem
    /** Model reasoning data. */
    public data class Reasoning(public val value: JsonObject) : ResponseOutputItem
    /** Hosted or future item with no promoted semantics. */
    public data class Extension(public val discriminator: String, public val value: JsonObject) : ResponseOutputItem
}

/** Official response envelope. */
public data class ResponseEnvelope(
    public val id: String,
    public val model: String,
    public val status: String?,
    public val output: List<ResponseOutputItem>,
    public val usage: JsonObject?,
    public val raw: JsonObject,
) {
    public companion object {
        /** Parses required official fields and portable output variants. */
        public fun parse(value: JsonObject): ResponseEnvelope {
            require(value.text("object") == "response")
            val id = value.text("id"); val model = value.text("model")
            val output = value.array("output").map { element ->
                val item = element.jsonObject
                when (val type = item.text("type")) {
                    "message" -> ResponseOutputItem.Message(item)
                    "function_call" -> ResponseOutputItem.FunctionCall(item)
                    "reasoning" -> ResponseOutputItem.Reasoning(item)
                    else -> ResponseOutputItem.Extension(type, item)
                }
            }
            return ResponseEnvelope(id, model, value["status"]?.jsonPrimitive?.contentOrNull, output, value["usage"] as? JsonObject, value)
        }
    }
}

/** Official error envelope with an open protocol type. */
public data class ErrorEnvelope(public val type: String, public val message: String, public val raw: JsonObject) {
    public companion object {
        /** Parses the standard outer `error` object. */
        public fun parse(value: JsonObject): ErrorEnvelope {
            val error = value.getValue("error").jsonObject
            return ErrorEnvelope(error.text("type"), error.text("message"), value)
        }
    }
}

/** Ordinary HTTP result without provider classification. */
public sealed interface DecodedResponse {
    /** Successful response fact. */
    public data class Response(public val status: Int, public val headers: List<ProtocolHeader>, public val response: ResponseEnvelope) : DecodedResponse
    /** Non-success error fact. */
    public data class Error(public val status: Int, public val headers: List<ProtocolHeader>, public val error: ErrorEnvelope) : DecodedResponse
}

/** Incremental UTF-8 SSE frame. */
public data class SseFrame(public val event: String?, public val data: String, public val id: String?, public val retry: ULong?)

/** Byte-chunk incremental SSE decoder. */
public class SseDecoder {
    private var buffer: ByteArray = byteArrayOf()

    /** Appends arbitrary transport bytes and emits complete frames. */
    public fun push(bytes: ByteArray): List<SseFrame> {
        buffer += bytes
        val frames = mutableListOf<SseFrame>()
        while (true) {
            val boundary = findBoundary(buffer) ?: break
            val frame = parseFrame(buffer.copyOfRange(0, boundary.first))
            buffer = buffer.copyOfRange(boundary.first + boundary.second, buffer.size)
            if (frame != null) frames += frame
        }
        return frames
    }

    /** Requires EOF to follow a complete frame or comments only. */
    public fun finish(): Unit {
        val trailing = runCatching { buffer.decodeToString(throwOnInvalidSequence = true) }.getOrNull()
        require(trailing != null && trailing.lineSequence().all { it.isBlank() || it.startsWith(':') })
        buffer = byteArrayOf()
    }
}

/** Typed Responses event retaining its complete original object. */
public data class ResponseStreamEvent(public val type: String, public val sequenceNumber: ULong, public val raw: JsonObject)

/** Incremental Responses lifecycle decoder. */
public class ResponsesStreamDecoder {
    private val sse: SseDecoder = SseDecoder()
    private var previous: ULong? = null
    private var created: Boolean = false
    private var terminal: Boolean = false

    /** Appends bytes and emits complete validated protocol events. */
    public fun push(bytes: ByteArray): List<ResponseStreamEvent> = sse.push(bytes).map { frame ->
        val value = JSON.parseToJsonElement(frame.data).jsonObject
        val type = value.text("type")
        require(frame.event == null || frame.event == type)
        val sequence = value.getValue("sequence_number").jsonPrimitive.content.toULong()
        require(previous == null || sequence > previous!!); previous = sequence
        require(!terminal)
        if (type == "response.created") { require(!created); created = true } else require(created)
        if (type in TERMINALS) terminal = true
        ResponseStreamEvent(type, sequence, value)
    }

    /** Requires exactly one protocol terminal at EOF. */
    public fun finish(): Unit { sse.finish(); require(terminal) }
}

private val JSON: Json = Json { ignoreUnknownKeys = false }
private val TYPED_REQUEST_FIELDS: Set<String> = setOf("model", "input", "stream", "max_output_tokens", "temperature", "top_p", "tools", "tool_choice", "text", "reasoning", "metadata")
private val TERMINALS: Set<String> = setOf("response.completed", "response.failed", "response.incomplete")

private fun JsonObject.text(name: String): String = getValue(name).jsonPrimitive.content
private fun JsonObject.array(name: String): JsonArray = getValue(name) as JsonArray
private fun requireJsonMedia(headers: List<ProtocolHeader>): Unit {
    val media = headers.firstOrNull { it.name == "content-type" }?.value ?: "application/json"
    require(media.substringBefore(';') == "application/json")
}

private fun findBoundary(bytes: ByteArray): Pair<Int, Int>? {
    for (index in bytes.indices) {
        if (index + 1 < bytes.size && bytes[index] == 10.toByte() && bytes[index + 1] == 10.toByte()) return index to 2
        if (index + 3 < bytes.size && bytes[index] == 13.toByte() && bytes[index + 1] == 10.toByte() && bytes[index + 2] == 13.toByte() && bytes[index + 3] == 10.toByte()) return index to 4
    }
    return null
}

private fun parseFrame(bytes: ByteArray): SseFrame? {
    val lines = bytes.decodeToString(throwOnInvalidSequence = true).lineSequence()
    var event: String? = null; var id: String? = null; var retry: ULong? = null
    val data = mutableListOf<String>()
    lines.forEach { raw ->
        val line = raw.removeSuffix("\r")
        if (line.isNotEmpty() && !line.startsWith(':')) {
            val field = line.substringBefore(':'); val value = line.substringAfter(':', "").removePrefix(" ")
            when (field) { "event" -> event = value; "data" -> data += value; "id" -> if ('\u0000' !in value) id = value; "retry" -> retry = value.toULong() }
        }
    }
    return if (data.isEmpty()) null else SseFrame(event, data.joinToString("\n"), id, retry)
}

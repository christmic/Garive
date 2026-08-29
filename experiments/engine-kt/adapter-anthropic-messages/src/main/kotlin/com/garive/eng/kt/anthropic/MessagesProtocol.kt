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

/** Protocol-only Messages adapter; it owns no retry or model mapping. */
public class MessagesAdapter(public val config: MessagesAdapterConfig) {
    /** Encodes a validated official create request. */
    public fun prepare(request: CreateMessageRequest): ProtocolHttpRequest {
        request.validate()
        val body = request.toJson()
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

private val JSON: Json = Json { ignoreUnknownKeys = false }
private fun JsonObject.text(name: String): String = getValue(name).jsonPrimitive.content
private fun JsonObject.array(name: String): JsonArray = getValue(name) as JsonArray
private fun requireJsonMedia(headers: List<ProtocolHeader>): Unit {
    require((headers.firstOrNull { it.name == "content-type" }?.value ?: "application/json").substringBefore(';') == "application/json")
}

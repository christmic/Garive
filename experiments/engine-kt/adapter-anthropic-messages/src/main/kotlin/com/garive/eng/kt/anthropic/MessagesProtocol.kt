package com.garive.eng.kt.anthropic

import java.net.URI
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject

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
    public fun prepare(request: CreateMessageRequest): ProtocolHttpRequest = messageFailure(MessagesProtocolError.INVALID_REQUEST) {
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
    public fun decodeResponse(status: Int, headers: List<ProtocolHeader>, body: ByteArray): DecodedResponse = messageFailure(MessagesProtocolError.INVALID_JSON) {
        requireJsonMedia(headers)
        val value = JSON.parseToJsonElement(body.decodeToString()).jsonObject
        return if (status in 200..299) DecodedResponse.Message(status, headers, MessageResponse.parse(value))
        else DecodedResponse.Error(status, headers, ErrorEnvelope.parse(value))
    }
}

private val JSON: Json = Json { ignoreUnknownKeys = false }
private fun requireJsonMedia(headers: List<ProtocolHeader>): Unit {
    if ((headers.firstOrNull { it.name == "content-type" }?.value ?: "application/json").substringBefore(';') != "application/json") {
        throw MessagesProtocolException(MessagesProtocolError.INVALID_MEDIA_TYPE)
    }
}

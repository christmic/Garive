package com.garive.eng.kt.openai

import java.net.URI
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject

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
        require(headers.none { it.name in setOf(HttpWire.HEADER_CONTENT_TYPE, HttpWire.HEADER_ACCEPT) })
    }
}

/** One complete request for a Runtime-owned HTTP transport. */
public class ProtocolHttpRequest internal constructor(
    public val uri: String,
    public val headers: List<ProtocolHeader>,
    public val body: ByteArray,
) {
    /** Responses-compatible requests use POST. */
    public val method: String = HttpWire.METHOD_POST
}

/** Protocol-only Responses adapter. It performs exactly one wire exchange. */
public class ResponsesAdapter(public val config: ResponsesAdapterConfig) {
    /** Encodes a validated official create request. */
    public fun prepare(request: CreateResponseRequest): ProtocolHttpRequest = responseFailure(ResponsesProtocolError.INVALID_REQUEST) {
        request.validate()
        val body = request.toJson()
        val headers = config.headers + listOf(
            ProtocolHeader.create(HttpWire.HEADER_CONTENT_TYPE, HttpWire.MEDIA_JSON, false),
            ProtocolHeader.create(HttpWire.HEADER_ACCEPT, if (request.stream) HttpWire.MEDIA_SSE else HttpWire.MEDIA_JSON, false),
        )
        return ProtocolHttpRequest(config.endpoint, headers, body.toString().encodeToByteArray())
    }

    /** Decodes ordinary JSON while retaining HTTP status and headers as facts. */
    public fun decodeResponse(status: Int, headers: List<ProtocolHeader>, body: ByteArray): DecodedResponse = responseFailure(ResponsesProtocolError.INVALID_JSON) {
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

private val JSON: Json = Json { ignoreUnknownKeys = false }
private fun requireJsonMedia(headers: List<ProtocolHeader>): Unit {
    val media = headers.firstOrNull { it.name == HttpWire.HEADER_CONTENT_TYPE }?.value ?: HttpWire.MEDIA_JSON
    if (media.substringBefore(';') != HttpWire.MEDIA_JSON) {
        throw ResponsesProtocolException(ResponsesProtocolError.INVALID_MEDIA_TYPE)
    }
}

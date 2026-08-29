package com.garive.eng.kt.provider.anthropic

import com.garive.eng.kt.anthropic.CreateMessageRequest
import com.garive.eng.kt.anthropic.Message
import com.garive.eng.kt.anthropic.OutputConfig
import com.garive.eng.kt.anthropic.ProtocolHeader
import com.garive.eng.kt.anthropic.SystemPrompt
import com.garive.eng.kt.anthropic.ThinkingConfig
import com.garive.eng.kt.anthropic.Tool
import com.garive.eng.kt.anthropic.ToolChoice
import com.garive.eng.kt.anthropic.encodeCreateMessageRequest
import com.garive.eng.kt.provider.profile.ConnectionInput
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

/** Stable failures for the exact token-count capability. */
public enum class AnthropicTokenCountError(public val code: String) {
    INVALID_REQUEST("invalid_request"),
    UNSUPPORTED_EXTENSION("unsupported_extension"),
    INVALID_RESPONSE("invalid_response"),
}

/** Exception carrying one secret-free token-count [error]. */
public class AnthropicTokenCountException(public val error: AnthropicTokenCountError) :
    IllegalArgumentException(error.code)

/** Exact official token-count request projection. */
public class CountTokensRequest internal constructor(
    public val model: String,
    public val messages: List<Message>,
    public val system: SystemPrompt?,
    public val tools: List<Tool>,
    public val toolChoice: ToolChoice?,
    public val outputConfig: OutputConfig?,
    public val thinking: ThinkingConfig?,
    internal val wire: JsonObject,
)

/** Projects a validated create request without generation-only fields. */
public fun projectTokenCountRequest(request: CreateMessageRequest): CountTokensRequest = tokenCountFailure(
    AnthropicTokenCountError.INVALID_REQUEST,
) {
    request.validate()
    if (request.extensions.isNotEmpty()) fail(AnthropicTokenCountError.UNSUPPORTED_EXTENSION)
    val encoded = encodeCreateMessageRequest(request)
    val retained = setOf("model", "messages", "system", "tools", "tool_choice", "output_config", "thinking")
    val wire = JsonObject(encoded.filterKeys { it in retained })
    CountTokensRequest(
        request.model,
        request.messages,
        request.system,
        request.tools,
        request.toolChoice,
        request.outputConfig,
        request.thinking,
        wire,
    )
}

/** Positive exact provider-reported input token count. */
@JvmInline
public value class TokenCount private constructor(public val inputTokens: ULong) {
    public companion object {
        internal fun create(value: ULong): TokenCount {
            if (value == 0uL) fail(AnthropicTokenCountError.INVALID_RESPONSE)
            return TokenCount(value)
        }
    }
}

/** Decodes the exact successful response shape. */
public fun decodeTokenCount(body: ByteArray): TokenCount = tokenCountFailure(
    AnthropicTokenCountError.INVALID_RESPONSE,
) {
    val text = body.decodeToString(throwOnInvalidSequence = true)
    if (Regex("\\\"input_tokens\\\"\\s*:").findAll(text).count() != 1) {
        fail(AnthropicTokenCountError.INVALID_RESPONSE)
    }
    val value = JSON.parseToJsonElement(text).jsonObject
    if (value.keys != setOf("input_tokens")) fail(AnthropicTokenCountError.INVALID_RESPONSE)
    val count = value["input_tokens"]!!.jsonPrimitive.content.toULongOrNull()
        ?: fail(AnthropicTokenCountError.INVALID_RESPONSE)
    TokenCount.create(count)
}

/** Explicit vendor profile for one token-count exchange. */
public class AnthropicTokenCountProfile internal constructor(
    public val endpoint: String,
    public val headers: List<ProtocolHeader>,
) {
    /** Encodes one request without executing HTTP. */
    public fun prepare(request: CountTokensRequest): TokenCountHttpRequest =
        TokenCountHttpRequest(endpoint, headers, request.wire.toString().encodeToByteArray())

    override fun toString(): String = "AnthropicTokenCountProfile(endpoint=$endpoint, headers=$headers)"
}

/** Builds the capability only from explicit Runtime-supplied values. */
public fun buildAnthropicTokenCountProfile(input: ConnectionInput): AnthropicTokenCountProfile {
    val connection = input.resolve(Constants.TOKEN_COUNT_DEFAULT_ENDPOINT, Constants.RESERVED_HEADERS)
    val headers = connection.extraHeaders.map { ProtocolHeader.create(it.name, it.value, it.sensitive) } + listOf(
        ProtocolHeader.create(Constants.API_KEY, connection.credential.exposeSecret(), true),
        ProtocolHeader.create(Constants.VERSION_HEADER, Constants.PROTOCOL_VERSION, false),
        ProtocolHeader.create(Constants.CONTENT_TYPE, Constants.MEDIA_JSON, false),
        ProtocolHeader.create(Constants.ACCEPT, Constants.MEDIA_JSON, false),
    )
    return AnthropicTokenCountProfile(connection.endpoint, headers)
}

/** Fully described request for Runtime-owned HTTP transport. */
public class TokenCountHttpRequest internal constructor(
    public val uri: String,
    public val headers: List<ProtocolHeader>,
    public val body: ByteArray,
) {
    public val method: String = Constants.METHOD_POST
    override fun toString(): String =
        "TokenCountHttpRequest(uri=$uri, headers=$headers, bodyLength=${body.size})"
}

private val JSON: Json = Json { ignoreUnknownKeys = false }

private fun fail(error: AnthropicTokenCountError): Nothing = throw AnthropicTokenCountException(error)

private inline fun <T> tokenCountFailure(error: AnthropicTokenCountError, block: () -> T): T = try {
    block()
} catch (failure: AnthropicTokenCountException) {
    throw failure
} catch (_: Exception) {
    fail(error)
}

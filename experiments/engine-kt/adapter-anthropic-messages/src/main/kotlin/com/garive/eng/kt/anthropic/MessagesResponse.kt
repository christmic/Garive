package com.garive.eng.kt.anthropic

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long

/** Portable output content plus lossless hosted/future blocks. */
public sealed interface OutputBlock {
    /** Text output with lossless citations. */
    public data class Text(public val text: String, public val citations: List<JsonElement>?, public val raw: JsonObject) : OutputBlock
    /** Extended thinking. */
    public data class Thinking(public val thinking: String, public val signature: String, public val raw: JsonObject) : OutputBlock
    /** Opaque redacted thinking. */
    public data class RedactedThinking(public val data: String, public val raw: JsonObject) : OutputBlock
    /** Client tool invocation. */
    public data class ToolUse(
        public val id: String, public val name: String, public val input: JsonObject,
        public val caller: JsonElement?, public val raw: JsonObject,
    ) : OutputBlock
    /** Hosted or future output without promoted semantics. */
    public data class Extension(public val discriminator: String, public val value: JsonObject) : OutputBlock
}

/** Official portable stop reasons. */
public enum class StopReason { END_TURN, MAX_TOKENS, STOP_SEQUENCE, TOOL_USE, PAUSE_TURN, REFUSAL, MODEL_CONTEXT_WINDOW_EXCEEDED }

/** Prompt-cache token breakdown by TTL. */
public data class CacheCreation(public val oneHourInputTokens: ULong, public val fiveMinuteInputTokens: ULong, public val raw: JsonObject)

/** Server tool request counts retained as protocol data. */
public data class ServerToolUsage(public val webFetchRequests: ULong, public val webSearchRequests: ULong, public val raw: JsonObject)

/** Output token observability breakdown. */
public data class OutputTokensDetails(public val thinkingTokens: ULong, public val raw: JsonObject)

/** Official response usage without invented totals. */
public data class Usage(
    public val inputTokens: ULong, public val outputTokens: ULong,
    public val cacheCreation: CacheCreation?, public val cacheCreationInputTokens: ULong?,
    public val cacheReadInputTokens: ULong?, public val inferenceGeo: String?,
    public val outputTokensDetails: OutputTokensDetails?, public val serverToolUse: ServerToolUsage?,
    public val serviceTier: String?, public val raw: JsonObject,
)

/** Official ordinary Message response. */
public data class MessageResponse(
    public val id: String, public val model: String, public val stopReason: StopReason?,
    public val stopSequence: String?, public val stopDetails: JsonElement?,
    public val content: List<OutputBlock>, public val usage: Usage, public val raw: JsonObject,
) {
    public companion object {
        /** Parses required fields and portable output variants. */
        public fun parse(value: JsonObject): MessageResponse {
            require(value.text(MessageFields.TYPE) == MessageKinds.MESSAGE && value.text("role") == MessageKinds.ASSISTANT)
            val id = value.text("id"); val model = value.text("model")
            require(id.isNotEmpty() && model.isNotEmpty())
            return MessageResponse(
                id, model, value.stringOrNull("stop_reason")?.enumWire(), value.stringOrNull("stop_sequence"),
                value["stop_details"]?.takeUnless { it is JsonNull }, value.array("content").map(::parseOutputBlock),
                parseUsage(value.getValue("usage").jsonObject), value,
            )
        }
    }
}

/** Official error envelope with an open error type. */
public data class ErrorEnvelope(public val type: String, public val message: String, public val requestId: String?, public val raw: JsonObject) {
    public companion object {
        /** Parses the standard outer error object. */
        public fun parse(value: JsonObject): ErrorEnvelope {
            require(value.text(MessageFields.TYPE) == MessageKinds.ERROR)
            val error = value.getValue(MessageFields.ERROR).jsonObject
            return ErrorEnvelope(error.text(MessageFields.TYPE), error.text("message"), value.stringOrNull("request_id"), value)
                .also { require(it.type.isNotEmpty() && it.message.isNotEmpty()) }
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

private fun parseOutputBlock(element: JsonElement): OutputBlock {
    val block = element.jsonObject
    return when (val type = block.text(MessageFields.TYPE)) {
        MessageKinds.TEXT -> OutputBlock.Text(block.text("text").also(String::requireNotEmpty), (block["citations"] as? JsonArray)?.toList(), block)
        MessageKinds.THINKING -> OutputBlock.Thinking(block.text("thinking").also(String::requireNotEmpty), block.text("signature").also(String::requireNotEmpty), block)
        MessageKinds.REDACTED_THINKING -> OutputBlock.RedactedThinking(block.text("data").also(String::requireNotEmpty), block)
        MessageKinds.TOOL_USE -> OutputBlock.ToolUse(block.text("id").also(String::requireNotEmpty), block.text("name").also(String::requireNotEmpty), block.getValue("input").jsonObject, block["caller"]?.takeUnless { it is JsonNull }, block)
        else -> OutputBlock.Extension(type.also(String::requireNotEmpty), block)
    }
}

private fun parseUsage(value: JsonObject): Usage = Usage(
    value.ulong("input_tokens"), value.ulong("output_tokens"),
    value.objectOrNull("cache_creation")?.let { CacheCreation(it.ulong("ephemeral_1h_input_tokens"), it.ulong("ephemeral_5m_input_tokens"), it) },
    value.ulongOrNull("cache_creation_input_tokens"), value.ulongOrNull("cache_read_input_tokens"), value.stringOrNull("inference_geo"),
    value.objectOrNull("output_tokens_details")?.let { OutputTokensDetails(it.ulong("thinking_tokens"), it) },
    value.objectOrNull("server_tool_use")?.let { ServerToolUsage(it.ulong("web_fetch_requests"), it.ulong("web_search_requests"), it) },
    value.stringOrNull("service_tier"), value,
)
private fun String.requireNotEmpty(): Unit = require(isNotEmpty())
private fun JsonObject.text(name: String): String = getValue(name).jsonPrimitive.content
private fun JsonObject.array(name: String): JsonArray = getValue(name) as JsonArray
private fun JsonObject.objectOrNull(name: String): JsonObject? = get(name)?.takeUnless { it is JsonNull } as? JsonObject
private fun JsonObject.stringOrNull(name: String): String? = get(name)?.takeUnless { it is JsonNull }?.jsonPrimitive?.contentOrNull
private fun JsonObject.ulong(name: String): ULong = getValue(name).jsonPrimitive.long.also { require(it >= 0) }.toULong()
private fun JsonObject.ulongOrNull(name: String): ULong? = get(name)?.takeUnless { it is JsonNull }?.jsonPrimitive?.long?.also { require(it >= 0) }?.toULong()
private inline fun <reified T : Enum<T>> String.enumWire(): T = enumValues<T>().first { it.name.lowercase() == this }

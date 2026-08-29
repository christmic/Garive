package com.garive.eng.kt.openai

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.double
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long

/** Typed portable response output item with lossless extensions. */
public sealed interface ResponseOutputItem {
    /** Assistant message output. */
    public data class Message(
        public val id: String,
        public val status: ItemStatus,
        public val content: List<OutputContent>,
        public val phase: String?,
        public val raw: JsonObject,
    ) : ResponseOutputItem
    /** Client function call. */
    public data class FunctionCall(
        public val id: String?, public val callId: String, public val name: String,
        public val arguments: String, public val status: ItemStatus?, public val raw: JsonObject,
    ) : ResponseOutputItem
    /** Model reasoning data. */
    public data class Reasoning(
        public val id: String, public val summary: List<ReasoningPart>,
        public val content: List<ReasoningPart>?, public val encryptedContent: String?,
        public val status: ItemStatus?, public val raw: JsonObject,
    ) : ResponseOutputItem
    /** Hosted or future item with no promoted semantics. */
    public data class Extension(public val discriminator: String, public val value: JsonObject) : ResponseOutputItem
}

/** Portable assistant output content. */
public sealed interface OutputContent {
    /** Complete generated text. */
    public data class Text(
        public val text: String, public val annotations: List<JsonElement>,
        public val logprobs: List<JsonElement>, public val raw: JsonObject,
    ) : OutputContent
    /** Complete refusal. */
    public data class Refusal(public val refusal: String, public val raw: JsonObject) : OutputContent
    /** Future content retained losslessly. */
    public data class Extension(public val discriminator: String, public val value: JsonObject) : OutputContent
}

/** Reasoning summary or visible reasoning part. */
public data class ReasoningPart(public val type: String, public val text: String, public val raw: JsonObject)

/** Official response lifecycle values. */
public enum class ResponseStatus { QUEUED, IN_PROGRESS, COMPLETED, FAILED, CANCELLED, INCOMPLETE }

/** Response-attached generation error. */
public data class ResponseError(public val code: String, public val message: String, public val raw: JsonObject)

/** Incomplete response detail. */
public data class IncompleteDetails(public val reason: String, public val raw: JsonObject)

/** Official input token details. */
public data class InputTokenDetails(public val cachedTokens: ULong, public val cacheWriteTokens: ULong, public val raw: JsonObject)

/** Official output token details. */
public data class OutputTokenDetails(public val reasoningTokens: ULong, public val raw: JsonObject)

/** Official checked token usage. */
public data class ResponseUsage(
    public val inputTokens: ULong, public val inputTokensDetails: InputTokenDetails,
    public val outputTokens: ULong, public val outputTokensDetails: OutputTokenDetails,
    public val totalTokens: ULong, public val raw: JsonObject,
)

/** Official response envelope. */
public data class ResponseEnvelope(
    public val id: String, public val createdAt: Double, public val model: String,
    public val status: ResponseStatus?, public val error: ResponseError?,
    public val incompleteDetails: IncompleteDetails?, public val output: List<ResponseOutputItem>,
    public val instructions: JsonElement?, public val metadata: Map<String, String>?,
    public val parallelToolCalls: Boolean, public val toolChoice: JsonElement,
    public val tools: List<JsonElement>, public val temperature: Double?, public val topP: Double?,
    public val text: JsonElement?, public val usage: ResponseUsage?, public val raw: JsonObject,
) {
    public companion object {
        /** Parses and validates the official portable response envelope. */
        public fun parse(value: JsonObject): ResponseEnvelope {
            require(value.text("object") == ResponseKinds.RESPONSE)
            val id = value.text("id"); val model = value.text("model"); val created = value.getValue("created_at").jsonPrimitive.double
            require(id.isNotEmpty() && model.isNotEmpty() && created.isFinite() && created >= 0.0)
            val error = value.objectOrNull(ResponseFields.ERROR)?.let { ResponseError(it.text("code"), it.text("message"), it) }
            val incomplete = value.objectOrNull("incomplete_details")?.let { IncompleteDetails(it.text("reason"), it) }
            val status = value.stringOrNull("status")?.enumWire<ResponseStatus>()
            require(status != ResponseStatus.COMPLETED || error == null)
            return ResponseEnvelope(
                id, created, model, status, error, incomplete,
                value.array("output").map(::parseOutputItem),
                value["instructions"]?.takeUnless { it is JsonNull }, value.objectOrNull("metadata")?.mapValues { it.value.jsonPrimitive.content },
                value.getValue("parallel_tool_calls").jsonPrimitive.content.toBooleanStrict(),
                value.getValue("tool_choice"), value.array("tools"), value.doubleOrNull("temperature"), value.doubleOrNull("top_p"),
                value["text"]?.takeUnless { it is JsonNull }, value.objectOrNull("usage")?.let(::parseUsage), value,
            )
        }
    }
}

/** Official error envelope with open type and code values. */
public data class ErrorEnvelope(
    public val type: String, public val message: String, public val code: String?,
    public val param: String?, public val raw: JsonObject,
) {
    public companion object {
        /** Parses the standard outer `error` object. */
        public fun parse(value: JsonObject): ErrorEnvelope {
            val error = value.getValue(ResponseFields.ERROR).jsonObject
            return ErrorEnvelope(error.text(ResponseFields.TYPE), error.text("message"), error.stringOrNull("code"), error.stringOrNull("param"), value)
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

private fun parseOutputItem(element: JsonElement): ResponseOutputItem {
    val item = element.jsonObject
    return when (val type = item.text(ResponseFields.TYPE)) {
        ResponseKinds.MESSAGE -> {
            require(item.text(ResponseFields.ROLE) == ResponseKinds.ASSISTANT)
            ResponseOutputItem.Message(item.text("id").also(String::requireNotEmpty), item.text("status").enumWire(), item.array("content").map(::parseContent), item.stringOrNull("phase"), item)
        }
        ResponseKinds.FUNCTION_CALL -> ResponseOutputItem.FunctionCall(item.stringOrNull("id"), item.text("call_id").also(String::requireNotEmpty), item.text("name").also(String::requireNotEmpty), item.text("arguments"), item.stringOrNull("status")?.enumWire(), item)
        ResponseKinds.REASONING -> ResponseOutputItem.Reasoning(item.text("id").also(String::requireNotEmpty), item.array("summary").map(::parseReasoning), (item["content"] as? JsonArray)?.map(::parseReasoning), item.stringOrNull("encrypted_content"), item.stringOrNull("status")?.enumWire(), item)
        else -> ResponseOutputItem.Extension(type, item)
    }
}

private fun parseContent(element: JsonElement): OutputContent {
    val content = element.jsonObject
    return when (val type = content.text(ResponseFields.TYPE)) {
        ResponseKinds.OUTPUT_TEXT -> OutputContent.Text(content.text("text"), content.array("annotations"), (content["logprobs"] as? JsonArray)?.toList() ?: emptyList(), content)
        ResponseKinds.REFUSAL -> OutputContent.Refusal(content.text("refusal"), content)
        else -> OutputContent.Extension(type, content)
    }
}

private fun parseReasoning(element: JsonElement): ReasoningPart = element.jsonObject.let { ReasoningPart(it.text(ResponseFields.TYPE), it.text("text"), it) }
private fun parseUsage(value: JsonObject): ResponseUsage {
    val input = value.getValue("input_tokens").jsonPrimitive.long.toULong()
    val output = value.getValue("output_tokens").jsonPrimitive.long.toULong()
    val total = value.getValue("total_tokens").jsonPrimitive.long.toULong()
    require(input + output == total)
    val inputDetails = value.getValue("input_tokens_details").jsonObject
    val outputDetails = value.getValue("output_tokens_details").jsonObject
    return ResponseUsage(input, InputTokenDetails(inputDetails.ulong("cached_tokens"), inputDetails.ulong("cache_write_tokens"), inputDetails), output, OutputTokenDetails(outputDetails.ulong("reasoning_tokens"), outputDetails), total, value)
}
private fun JsonObject.text(name: String): String = getValue(name).jsonPrimitive.content
private fun JsonObject.array(name: String): JsonArray = getValue(name) as JsonArray
private fun JsonObject.objectOrNull(name: String): JsonObject? = get(name)?.takeUnless { it is JsonNull } as? JsonObject
private fun JsonObject.stringOrNull(name: String): String? = get(name)?.takeUnless { it is JsonNull }?.jsonPrimitive?.contentOrNull
private fun JsonObject.ulong(name: String): ULong = getValue(name).jsonPrimitive.long.also { require(it >= 0) }.toULong()
private fun JsonObject.doubleOrNull(name: String): Double? = get(name)?.takeUnless { it is JsonNull }?.jsonPrimitive?.double?.also { require(it.isFinite()) }
private fun String.requireNotEmpty(): Unit = require(isNotEmpty())
private inline fun <reified T : Enum<T>> String.enumWire(): T = enumValues<T>().first { it.name.lowercase() == this }

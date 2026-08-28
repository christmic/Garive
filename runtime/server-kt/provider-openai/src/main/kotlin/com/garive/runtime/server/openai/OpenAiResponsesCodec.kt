package com.garive.runtime.server.openai

import com.garive.runtime.server.llm.*
import kotlinx.serialization.json.*

enum class OpenAiAdapterError { INVALID_REQUEST, UNSUPPORTED_CAPABILITY, INVALID_JSON, INVARIANT }
sealed interface OpenAiResult<out T> {
    data class Success<T>(val value: T) : OpenAiResult<T>
    data class Failure(val error: OpenAiAdapterError) : OpenAiResult<Nothing>
}

object OpenAiResponsesCodec {
    fun renderRequest(request: ModelRequest, stream: Boolean): OpenAiResult<JsonObject> = guard {
        if (request.validate() != null || request.traceMetadata.size > 16) fail(OpenAiAdapterError.INVALID_REQUEST)
        val input = request.inputItems.map { item ->
            val message = item as? ModelInputItem.Message ?: fail(OpenAiAdapterError.UNSUPPORTED_CAPABILITY)
            buildJsonObject {
                put("type", "message")
                put("role", message.role.name.lowercase())
                putJsonArray("content") { message.content.forEach { content -> add(inputContent(content)) } }
            }
        }
        buildJsonObject {
            put("model", request.targetId.value); put("input", JsonArray(input)); put("stream", stream); put("store", false)
            request.output.maxOutputTokens?.let {
                if (it > Long.MAX_VALUE.toULong()) fail(OpenAiAdapterError.INVALID_REQUEST)
                put("max_output_tokens", it.toLong())
            }
            if (request.tools.isNotEmpty()) putJsonArray("tools") { request.tools.forEach { tool ->
                val schema = parse(tool.inputSchemaJson, OpenAiAdapterError.INVALID_REQUEST)
                add(buildJsonObject { put("type", "function"); put("name", tool.name); put("description", tool.description)
                    put("parameters", schema); put("strict", tool.strict) })
            } }
            if (request.traceMetadata.isNotEmpty()) putJsonObject("metadata") {
                request.traceMetadata.forEach { (key, value) -> put(key, value) }
            }
            when (val mode = request.output.textMode) {
                TextMode.Plain -> Unit
                TextMode.JsonObject -> putJsonObject("text") { putJsonObject("format") { put("type", "json_object") } }
                is TextMode.JsonSchema -> putJsonObject("text") { putJsonObject("format") {
                    put("type", "json_schema"); put("name", "garive_output")
                    put("schema", parse(mode.schemaJson, OpenAiAdapterError.INVALID_REQUEST)); put("strict", true)
                } }
            }
        }
    }

    fun parseResponse(bytes: ByteArray): OpenAiResult<InvokeOutcome> = guard { response(parse(bytes.decodeToString())) }

    fun parseSse(bytes: ByteArray): OpenAiResult<InvokeOutcome> = guard {
        var previous: ULong? = null
        val assembled = sortedMapOf<UInt, StringBuilder>()
        var terminal: InvokeOutcome? = null
        bytes.decodeToString().lineSequence().filter { it.startsWith("data: ") }.forEach { line ->
            if (terminal != null) fail(OpenAiAdapterError.INVARIANT)
            val event = parse(line.removePrefix("data: ")).jsonObject
            val sequence = event.ulong("sequence_number")
            if (previous?.let { sequence <= it } == true) fail(OpenAiAdapterError.INVARIANT)
            previous = sequence
            when (event.text("type")) {
                "response.output_text.delta" -> assembled.getOrPut(event.uint("output_index")) { StringBuilder() }
                    .append(event.text("delta"))
                "response.output_text.done" -> if (assembled[event.uint("output_index")]?.toString() != event.text("text"))
                    fail(OpenAiAdapterError.INVARIANT)
                "response.completed" -> terminal = response(event.getValue("response"))
                "response.failed" -> fail(OpenAiAdapterError.INVARIANT)
            }
        }
        terminal ?: InvokeOutcome.Interrupted(InterruptionKind.TRANSPORT,
            assembled.values.map { ModelItem.Text(it.toString()) }, unknownUsage())
    }

    private fun response(element: JsonElement): InvokeOutcome {
        val value = element.jsonObject
        if (value.text("status") != "completed") fail(OpenAiAdapterError.INVARIANT)
        val items = mutableListOf<ModelItem>()
        value.getValue("output").jsonArray.forEach { output ->
            val item = output.jsonObject
            when (item.text("type")) {
                "message" -> item.getValue("content").jsonArray.forEach { content ->
                    val part = content.jsonObject
                    items += when (part.text("type")) {
                        "output_text" -> ModelItem.Text(part.text("text"))
                        "refusal" -> ModelItem.Refusal(part.text("refusal"))
                        else -> fail(OpenAiAdapterError.UNSUPPORTED_CAPABILITY)
                    }
                }
                "function_call" -> items += ModelItem.ToolIntent(item.text("call_id"), item.text("name"), item.text("arguments"))
                "reasoning" -> item["encrypted_content"]?.jsonPrimitive?.contentOrNull?.let {
                    items += ModelItem.Reasoning(ReasoningContent.OpaqueReference(it))
                }
                else -> fail(OpenAiAdapterError.UNSUPPORTED_CAPABILITY)
            }
        }
        val usage = usage(value.getValue("usage").jsonObject)
        val stop = when { items.any { it is ModelItem.ToolIntent } -> ModelStopReason.ToolUse
            items.any { it is ModelItem.Refusal } -> ModelStopReason.Refusal else -> ModelStopReason.EndTurn }
        return InvokeOutcome.Completed(items, usage, stop)
    }

    private fun usage(value: JsonObject): ModelUsage {
        val input = value.ulong("input_tokens"); val output = value.ulong("output_tokens")
        if (input > ULong.MAX_VALUE - output || value.ulong("total_tokens") != input + output) fail(OpenAiAdapterError.INVARIANT)
        val details = value["input_tokens_details"]?.jsonObject
        return ModelUsage(TokenCount.Known(input), TokenCount.Known(output),
            details?.get("cached_tokens")?.jsonPrimitive?.content?.toULong()?.let(TokenCount::Known),
            details?.get("cache_write_tokens")?.jsonPrimitive?.content?.toULong()?.let(TokenCount::Known), UsageSource.PROVIDER_REPORTED)
    }
    private fun inputContent(value: ModelInputContent): JsonObject = when (value) {
        is ModelInputContent.Text -> buildJsonObject { put("type", "input_text"); put("text", value.text) }
        is ModelInputContent.MediaReference -> if (value.mediaKind == MediaKind.Image)
            buildJsonObject { put("type", "input_image"); put("image_url", value.reference) }
        else fail(OpenAiAdapterError.UNSUPPORTED_CAPABILITY)
    }
}

private class CodecFailure(val error: OpenAiAdapterError) : RuntimeException()
private fun fail(error: OpenAiAdapterError): Nothing = throw CodecFailure(error)
private inline fun <T> guard(block: () -> T): OpenAiResult<T> = try { OpenAiResult.Success(block()) }
catch (error: CodecFailure) { OpenAiResult.Failure(error.error) }
catch (_: IllegalArgumentException) { OpenAiResult.Failure(OpenAiAdapterError.INVALID_JSON) }
private fun parse(text: String, error: OpenAiAdapterError = OpenAiAdapterError.INVALID_JSON): JsonElement =
    try { Json.parseToJsonElement(text) } catch (_: IllegalArgumentException) { fail(error) }
private fun JsonObject.text(key: String) = getValue(key).jsonPrimitive.content
private fun JsonObject.ulong(key: String) = text(key).toULongOrNull() ?: fail(OpenAiAdapterError.INVARIANT)
private fun JsonObject.uint(key: String) = text(key).toUIntOrNull() ?: fail(OpenAiAdapterError.INVARIANT)
private fun unknownUsage() = ModelUsage(TokenCount.Unknown, TokenCount.Unknown, source = UsageSource.PROVIDER_REPORTED)

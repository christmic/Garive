package com.garive.runtime.server.anthropic

import com.garive.runtime.server.llm.*
import kotlinx.serialization.json.*

enum class AnthropicAdapterError { INVALID_REQUEST, UNSUPPORTED_CAPABILITY, INVALID_JSON, INVARIANT }
sealed interface AnthropicResult<out T> { data class Success<T>(val value: T) : AnthropicResult<T>
    data class Failure(val error: AnthropicAdapterError) : AnthropicResult<Nothing> }
private sealed interface Block { data class Text(val value: StringBuilder) : Block
    data class Tool(val id: String, val name: String, val json: StringBuilder) : Block }

object AnthropicMessagesCodec {
    fun renderRequest(request: ModelRequest, stream: Boolean): AnthropicResult<JsonObject> = guard {
        if (request.validate() != null || request.output.textMode != TextMode.Plain) fail(AnthropicAdapterError.INVALID_REQUEST)
        val limit = request.output.maxOutputTokens ?: fail(AnthropicAdapterError.INVALID_REQUEST)
        if (limit > Long.MAX_VALUE.toULong()) fail(AnthropicAdapterError.INVALID_REQUEST)
        val system = mutableListOf<JsonElement>(); val messages = mutableListOf<JsonElement>(); var started = false
        request.inputItems.forEach { item ->
            val message = item as? ModelInputItem.Message ?: fail(AnthropicAdapterError.UNSUPPORTED_CAPABILITY)
            if (message.role == ModelRole.SYSTEM || message.role == ModelRole.DEVELOPER) {
                if (started) fail(AnthropicAdapterError.UNSUPPORTED_CAPABILITY)
                message.content.forEach { system += text(it) }
            } else { started = true; messages += buildJsonObject { put("role", message.role.name.lowercase())
                put("content", JsonArray(message.content.map(::text))) } }
        }
        val tools = request.tools.map { tool ->
            if (tool.strict) fail(AnthropicAdapterError.UNSUPPORTED_CAPABILITY)
            buildJsonObject { put("name", tool.name); put("description", tool.description)
                put("input_schema", parse(tool.inputSchemaJson, AnthropicAdapterError.INVALID_REQUEST)) }
        }
        buildJsonObject { put("model", request.targetId.value); put("max_tokens", limit.toLong())
            put("messages", JsonArray(messages)); put("stream", stream)
            if (system.isNotEmpty()) put("system", JsonArray(system)); if (tools.isNotEmpty()) put("tools", JsonArray(tools))
            if (request.traceMetadata.isNotEmpty()) { if (request.traceMetadata.size != 1 || request.traceMetadata[0].first != "user_id")
                fail(AnthropicAdapterError.UNSUPPORTED_CAPABILITY)
                putJsonObject("metadata") { put("user_id", request.traceMetadata[0].second) } }
        }
    }

    fun parseResponse(bytes: ByteArray): AnthropicResult<InvokeOutcome> = guard {
        val value = parse(bytes.decodeToString()).jsonObject
        InvokeOutcome.Completed(content(value.getValue("content").jsonArray), usage(value.getValue("usage").jsonObject),
            stop(value.text("stop_reason")))
    }

    fun parseSse(bytes: ByteArray): AnthropicResult<InvokeOutcome> = guard {
        val blocks = sortedMapOf<UInt, Pair<Block, Boolean>>(); var usage = unknown(); var reason: ModelStopReason? = null
        var started = false; var terminal = false
        bytes.decodeToString().lineSequence().filter { it.startsWith("data: ") }.forEach { line ->
            if (terminal) fail(AnthropicAdapterError.INVARIANT)
            val event = parse(line.removePrefix("data: ")).jsonObject
            when (event.text("type")) {
                "message_start" -> { if (started) fail(AnthropicAdapterError.INVARIANT); started = true
                    usage = usage(event.getValue("message").jsonObject.getValue("usage").jsonObject) }
                "content_block_start" -> { val index = event.uint("index"); if (index in blocks) fail(AnthropicAdapterError.INVARIANT)
                    val value = event.getValue("content_block").jsonObject
                    blocks[index] = (when (value.text("type")) { "text" -> Block.Text(StringBuilder(value.text("text")))
                        "tool_use" -> Block.Tool(value.text("id"), value.text("name"), StringBuilder())
                        else -> fail(AnthropicAdapterError.UNSUPPORTED_CAPABILITY) }) to false }
                "content_block_delta" -> { val index = event.uint("index"); val pair = blocks[index] ?: fail(AnthropicAdapterError.INVARIANT)
                    if (pair.second) fail(AnthropicAdapterError.INVARIANT); val delta = event.getValue("delta").jsonObject
                    when (val block = pair.first) { is Block.Text -> if (delta.text("type") == "text_delta") block.value.append(delta.text("text")) else fail(AnthropicAdapterError.INVARIANT)
                        is Block.Tool -> if (delta.text("type") == "input_json_delta") block.json.append(delta.text("partial_json")) else fail(AnthropicAdapterError.INVARIANT) } }
                "content_block_stop" -> { val index = event.uint("index"); val pair = blocks[index] ?: fail(AnthropicAdapterError.INVARIANT)
                    if (pair.second) fail(AnthropicAdapterError.INVARIANT); val block = pair.first
                    if (block is Block.Tool) parse(block.json.toString()); blocks[index] = block to true }
                "message_delta" -> { reason = stop(event.getValue("delta").jsonObject.text("stop_reason")); val output = event.getValue("usage").jsonObject.ulong("output_tokens")
                    val prior = (usage.outputTokens as? TokenCount.Known)?.value ?: 0u; if (output < prior) fail(AnthropicAdapterError.INVARIANT)
                    usage = usage.copy(outputTokens = TokenCount.Known(output)) }
                "message_stop" -> { if (blocks.values.any { !it.second } || reason == null) fail(AnthropicAdapterError.INVARIANT); terminal = true }
                "ping" -> Unit; "error" -> fail(AnthropicAdapterError.INVARIANT)
                else -> fail(AnthropicAdapterError.UNSUPPORTED_CAPABILITY)
            }
        }
        val items = blocks.values.map { item(it.first) }
        if (terminal) InvokeOutcome.Completed(items, usage, requireNotNull(reason))
        else InvokeOutcome.Interrupted(InterruptionKind.TRANSPORT, items, usage)
    }

    private fun content(values: JsonArray) = values.map { element -> val value = element.jsonObject; when (value.text("type")) {
        "text" -> ModelItem.Text(value.text("text")); "tool_use" -> ModelItem.ToolIntent(value.text("id"), value.text("name"), value.getValue("input").toString())
        "thinking" -> ModelItem.Reasoning(ReasoningContent.ModelVisible(value.text("thinking")))
        "redacted_thinking" -> ModelItem.Reasoning(ReasoningContent.OpaqueReference(value.text("data")))
        else -> fail(AnthropicAdapterError.UNSUPPORTED_CAPABILITY) } }
    private fun usage(value: JsonObject): ModelUsage { val base = value.ulong("input_tokens")
        val write = value["cache_creation_input_tokens"]?.jsonPrimitive?.content?.toULong() ?: 0u
        val read = value["cache_read_input_tokens"]?.jsonPrimitive?.content?.toULong() ?: 0u
        if (base > ULong.MAX_VALUE - write || base + write > ULong.MAX_VALUE - read) fail(AnthropicAdapterError.INVARIANT)
        return ModelUsage(TokenCount.Known(base + write + read), TokenCount.Known(value.ulong("output_tokens")),
            TokenCount.Known(read), TokenCount.Known(write), UsageSource.PROVIDER_REPORTED) }
    private fun item(block: Block): ModelItem = when (block) { is Block.Text -> ModelItem.Text(block.value.toString())
        is Block.Tool -> ModelItem.ToolIntent(block.id, block.name, block.json.toString()) }
    private fun stop(value: String): ModelStopReason = when (value) { "end_turn" -> ModelStopReason.EndTurn; "tool_use" -> ModelStopReason.ToolUse
        "stop_sequence" -> ModelStopReason.StopSequence; "pause_turn" -> ModelStopReason.PauseTurn; "refusal" -> ModelStopReason.Refusal
        else -> ModelStopReason.Other(value) }
    private fun text(value: ModelInputContent): JsonObject = if (value is ModelInputContent.Text)
        buildJsonObject { put("type", "text"); put("text", value.text) } else fail(AnthropicAdapterError.UNSUPPORTED_CAPABILITY)
}

private class Failure(val error: AnthropicAdapterError) : RuntimeException()
private fun fail(error: AnthropicAdapterError): Nothing = throw Failure(error)
private inline fun <T> guard(block: () -> T): AnthropicResult<T> = try { AnthropicResult.Success(block()) }
catch (error: Failure) { AnthropicResult.Failure(error.error) } catch (_: IllegalArgumentException) { AnthropicResult.Failure(AnthropicAdapterError.INVALID_JSON) }
private fun parse(value: String, error: AnthropicAdapterError = AnthropicAdapterError.INVALID_JSON) = try { Json.parseToJsonElement(value) }
catch (_: IllegalArgumentException) { fail(error) }
private fun JsonObject.text(key: String) = getValue(key).jsonPrimitive.content
private fun JsonObject.ulong(key: String) = text(key).toULongOrNull() ?: fail(AnthropicAdapterError.INVARIANT)
private fun JsonObject.uint(key: String) = text(key).toUIntOrNull() ?: fail(AnthropicAdapterError.INVARIANT)
private fun unknown() = ModelUsage(TokenCount.Unknown, TokenCount.Unknown, source = UsageSource.PROVIDER_REPORTED)

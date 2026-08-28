package com.garive.runtime.server.anthropic

import com.garive.runtime.server.llm.*
import java.time.Instant
import java.time.format.DateTimeFormatter
import kotlinx.serialization.json.*
import kotlin.time.Duration
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.Duration.Companion.seconds

enum class AnthropicAdapterError { INVALID_REQUEST, UNSUPPORTED_CAPABILITY, INVALID_JSON, INVARIANT }
sealed interface AnthropicResult<out T> { data class Success<T>(val value: T) : AnthropicResult<T>
    data class Failure(val error: AnthropicAdapterError) : AnthropicResult<Nothing> }
private sealed interface Block { data class Text(val value: StringBuilder) : Block
    data class Thinking(val value: StringBuilder, val signature: StringBuilder) : Block
    data class RedactedThinking(val data: String) : Block
    data class Tool(val id: String, val name: String, val json: StringBuilder) : Block }
sealed interface HttpErrorAction { data class Retry(val retryAfter: Duration?) : HttpErrorAction
    data class Terminal(val outcome: InvokeOutcome) : HttpErrorAction }
data class HttpRequestDescriptor(val method: String, val path: String,
    val headers: List<Pair<String, String>>, val body: ByteArray)
data class HttpResponseDescriptor(val status: Int, val retryAfter: String?, val body: ByteArray)
enum class TransportFailure { CONNECTION, TIMEOUT }
sealed interface TransportResult { data class Success(val response: HttpResponseDescriptor) : TransportResult
    data class Failure(val reason: TransportFailure) : TransportResult }
interface AnthropicTransport { suspend fun execute(request: HttpRequestDescriptor,
    cancellation: ModelCancellation): TransportResult; suspend fun wait(delay: Duration) }
class AnthropicModelPort(private val transport: AnthropicTransport, private val maxAttempts: Int) : ModelPort {
    override suspend fun invoke(request: ModelRequest, observer: ModelObserver,
        cancellation: ModelCancellation): ModelPortResult {
        if (maxAttempts <= 0) return ModelPortResult.Failure(ModelPortFailure.INVALID_REQUEST)
        if (cancellation.isCancelled()) return ModelPortResult.Success(cancelled(null))
        for (attempt in 1..maxAttempts) {
            val descriptor = when (val rendered = AnthropicMessagesCodec.renderHttpRequest(request, true)) {
                is AnthropicResult.Success -> rendered.value
                is AnthropicResult.Failure -> return ModelPortResult.Failure(rendered.error.portFailure()) }
            val response = transport.execute(descriptor, cancellation)
            if (cancellation.isCancelled()) return ModelPortResult.Success(cancelled(null))
            val wire = when (response) { is TransportResult.Success -> response.response
                is TransportResult.Failure -> if (attempt < maxAttempts) { transport.wait(Duration.ZERO); continue }
                    else return ModelPortResult.Success(InvokeOutcome.Interrupted(
                        InterruptionKind.TRANSPORT, emptyList(), unknown())) }
            if (wire.status in 200..299) {
                val outcome = when (val parsed = AnthropicMessagesCodec.parseSse(wire.body)) {
                    is AnthropicResult.Success -> parsed.value
                    is AnthropicResult.Failure -> return ModelPortResult.Failure(parsed.error.portFailure()) }
                return ModelPortResult.Success(if (cancellation.isCancelled()) cancelled(outcome) else outcome)
            }
            when (val action = AnthropicMessagesCodec.classifyHttpError(wire.status, wire.retryAfter,
                wire.body, attempt == maxAttempts, Instant.now())) {
                is AnthropicResult.Failure -> return ModelPortResult.Failure(action.error.portFailure())
                is AnthropicResult.Success -> when (val value = action.value) {
                    is HttpErrorAction.Retry -> transport.wait(value.retryAfter ?: Duration.ZERO)
                    is HttpErrorAction.Terminal -> return ModelPortResult.Success(value.outcome) } }
        }
        return ModelPortResult.Failure(ModelPortFailure.ADAPTER_INVARIANT)
    }
}
private sealed interface ParsedStop { data class Completed(val reason: ModelStopReason) : ParsedStop
    data object OutputLimit : ParsedStop }

object AnthropicMessagesCodec {
    fun renderHttpRequest(request: ModelRequest, stream: Boolean): AnthropicResult<HttpRequestDescriptor> = guard {
        val body = when (val rendered = renderRequest(request, stream)) {
            is AnthropicResult.Success -> rendered.value.toString().encodeToByteArray()
            is AnthropicResult.Failure -> fail(rendered.error)
        }
        HttpRequestDescriptor("POST", "/v1/messages", listOf("content-type" to "application/json",
            "accept" to if (stream) "text/event-stream" else "application/json",
            "anthropic-version" to "2023-06-01"), body)
    }

    fun classifyHttpError(status: Int, retryAfter: String?, body: ByteArray, exhausted: Boolean, now: Instant): AnthropicResult<HttpErrorAction> = guard {
        val error = parse(body.decodeToString()).jsonObject.getValue("error").jsonObject
        val type = error["type"]?.jsonPrimitive?.contentOrNull.orEmpty()
        val message = error["message"]?.jsonPrimitive?.contentOrNull.orEmpty().lowercase()
        val evidence = "type:${type.take(64)}"
        if (type == "invalid_request_error" && ("prompt is too long" in message || "context window" in message))
            return@guard HttpErrorAction.Terminal(InvokeOutcome.Rejected(RejectionKind.CONTEXT_OVERFLOW, evidence))
        if (status == 401 || status == 403 || type == "authentication_error" || type == "permission_error")
            return@guard HttpErrorAction.Terminal(InvokeOutcome.Rejected(RejectionKind.AUTHENTICATION, evidence))
        val kind = when {
            status == 429 || type == "rate_limit_error" -> UnavailableKind.RATE_LIMITED
            status in setOf(500, 503, 504, 529) || type == "api_error" || type == "overloaded_error" -> UnavailableKind.MODEL_UNAVAILABLE
            else -> fail(AnthropicAdapterError.UNSUPPORTED_CAPABILITY)
        }
        val delay = retryAfter?.let { parseRetryAfter(it, now) }
        if (!exhausted) HttpErrorAction.Retry(delay) else HttpErrorAction.Terminal(InvokeOutcome.Unavailable(kind, delay))
    }

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
        val items = content(value.getValue("content").jsonArray); val usage = usage(value.getValue("usage").jsonObject)
        when (val stop = stop(value.text("stop_reason"))) {
            is ParsedStop.Completed -> InvokeOutcome.Completed(items, usage, stop.reason)
            ParsedStop.OutputLimit -> InvokeOutcome.Interrupted(InterruptionKind.OUTPUT_LIMIT, items, usage)
        }
    }

    fun parseSse(bytes: ByteArray): AnthropicResult<InvokeOutcome> = guard {
        val blocks = sortedMapOf<UInt, Pair<Block, Boolean>>(); var usage = unknown(); var reason: ParsedStop? = null
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
                        "thinking" -> Block.Thinking(StringBuilder(value.text("thinking")), StringBuilder())
                        "redacted_thinking" -> Block.RedactedThinking(value.text("data"))
                        "tool_use" -> Block.Tool(value.text("id"), value.text("name"), StringBuilder())
                        else -> fail(AnthropicAdapterError.UNSUPPORTED_CAPABILITY) }) to false }
                "content_block_delta" -> { val index = event.uint("index"); val pair = blocks[index] ?: fail(AnthropicAdapterError.INVARIANT)
                    if (pair.second) fail(AnthropicAdapterError.INVARIANT); val delta = event.getValue("delta").jsonObject
                    when (val block = pair.first) { is Block.Text -> if (delta.text("type") == "text_delta") block.value.append(delta.text("text")) else fail(AnthropicAdapterError.INVARIANT)
                        is Block.Thinking -> when (delta.text("type")) { "thinking_delta" -> block.value.append(delta.text("thinking"))
                            "signature_delta" -> block.signature.append(delta.text("signature")); else -> fail(AnthropicAdapterError.INVARIANT) }
                        is Block.RedactedThinking -> fail(AnthropicAdapterError.INVARIANT)
                        is Block.Tool -> if (delta.text("type") == "input_json_delta") block.json.append(delta.text("partial_json")) else fail(AnthropicAdapterError.INVARIANT) } }
                "content_block_stop" -> { val index = event.uint("index"); val pair = blocks[index] ?: fail(AnthropicAdapterError.INVARIANT)
                    if (pair.second) fail(AnthropicAdapterError.INVARIANT); val block = pair.first
                    if (block is Block.Tool) parse(block.json.toString()); blocks[index] = block to true }
                "message_delta" -> { reason = stop(event.getValue("delta").jsonObject.text("stop_reason")); val output = event.getValue("usage").jsonObject.ulong("output_tokens")
                    val prior = (usage.outputTokens as? TokenCount.Known)?.value ?: 0u; if (output < prior) fail(AnthropicAdapterError.INVARIANT)
                    usage = usage.copy(outputTokens = TokenCount.Known(output)) }
                "message_stop" -> { if (blocks.values.any { !it.second } || reason == null) fail(AnthropicAdapterError.INVARIANT); terminal = true }
                "ping" -> Unit; "error" -> {
                    val items = blocks.values.flatMap { items(it.first) }
                    if (items.isNotEmpty()) return@guard InvokeOutcome.Interrupted(InterruptionKind.TRANSPORT, items, usage)
                    return@guard streamError(event)
                }
                else -> fail(AnthropicAdapterError.UNSUPPORTED_CAPABILITY)
            }
        }
        val items = blocks.values.flatMap { items(it.first) }
        if (!terminal) InvokeOutcome.Interrupted(InterruptionKind.TRANSPORT, items, usage)
        else when (val stop = requireNotNull(reason)) { is ParsedStop.Completed -> InvokeOutcome.Completed(items, usage, stop.reason)
            ParsedStop.OutputLimit -> InvokeOutcome.Interrupted(InterruptionKind.OUTPUT_LIMIT, items, usage) }
    }

    private fun content(values: JsonArray) = values.flatMap { element -> val value = element.jsonObject; when (value.text("type")) {
        "text" -> listOf(ModelItem.Text(value.text("text"))); "tool_use" -> listOf(ModelItem.ToolIntent(value.text("id"), value.text("name"), value.getValue("input").toString()))
        "thinking" -> buildList { add(ModelItem.Reasoning(ReasoningContent.ModelVisible(value.text("thinking"))))
            value["signature"]?.jsonPrimitive?.contentOrNull?.let { add(ModelItem.Reasoning(ReasoningContent.OpaqueReference(it))) } }
        "redacted_thinking" -> listOf(ModelItem.Reasoning(ReasoningContent.OpaqueReference(value.text("data"))))
        else -> fail(AnthropicAdapterError.UNSUPPORTED_CAPABILITY) } }
    private fun usage(value: JsonObject): ModelUsage { val base = value.ulong("input_tokens")
        val write = value["cache_creation_input_tokens"]?.jsonPrimitive?.content?.toULong() ?: 0u
        val read = value["cache_read_input_tokens"]?.jsonPrimitive?.content?.toULong() ?: 0u
        if (base > ULong.MAX_VALUE - write || base + write > ULong.MAX_VALUE - read) fail(AnthropicAdapterError.INVARIANT)
        return ModelUsage(TokenCount.Known(base + write + read), TokenCount.Known(value.ulong("output_tokens")),
            TokenCount.Known(read), TokenCount.Known(write), UsageSource.PROVIDER_REPORTED) }
    private fun items(block: Block): List<ModelItem> = when (block) { is Block.Text -> listOf(ModelItem.Text(block.value.toString()))
        is Block.Thinking -> buildList { add(ModelItem.Reasoning(ReasoningContent.ModelVisible(block.value.toString())))
            if (block.signature.isNotEmpty()) add(ModelItem.Reasoning(ReasoningContent.OpaqueReference(block.signature.toString()))) }
        is Block.RedactedThinking -> listOf(ModelItem.Reasoning(ReasoningContent.OpaqueReference(block.data)))
        is Block.Tool -> listOf(ModelItem.ToolIntent(block.id, block.name, block.json.toString())) }
    private fun stop(value: String): ParsedStop = when (value) { "end_turn" -> ParsedStop.Completed(ModelStopReason.EndTurn)
        "tool_use" -> ParsedStop.Completed(ModelStopReason.ToolUse); "stop_sequence" -> ParsedStop.Completed(ModelStopReason.StopSequence)
        "pause_turn" -> ParsedStop.Completed(ModelStopReason.PauseTurn); "refusal" -> ParsedStop.Completed(ModelStopReason.Refusal)
        "max_tokens", "model_context_window_exceeded" -> ParsedStop.OutputLimit
        else -> ParsedStop.Completed(ModelStopReason.Other(value)) }
    private fun streamError(event: JsonObject): InvokeOutcome { val type = event.getValue("error").jsonObject.text("type")
        return when (type) { "authentication_error", "permission_error" -> InvokeOutcome.Rejected(RejectionKind.AUTHENTICATION, "type:${type.take(64)}")
            "rate_limit_error" -> InvokeOutcome.Unavailable(UnavailableKind.RATE_LIMITED, null)
            "api_error", "overloaded_error" -> InvokeOutcome.Unavailable(UnavailableKind.MODEL_UNAVAILABLE, null)
            else -> fail(AnthropicAdapterError.UNSUPPORTED_CAPABILITY) } }
    private fun text(value: ModelInputContent): JsonObject = if (value is ModelInputContent.Text)
        buildJsonObject { put("type", "text"); put("text", value.text) } else fail(AnthropicAdapterError.UNSUPPORTED_CAPABILITY)
}

private fun parseRetryAfter(value: String, now: Instant): Duration? { value.trim().toLongOrNull()?.let { if (it >= 0) return it.seconds }
    val deadline = runCatching { Instant.from(DateTimeFormatter.RFC_1123_DATE_TIME.parse(value)) }.getOrNull() ?: return null
    val millis = deadline.toEpochMilli() - now.toEpochMilli(); return if (millis >= 0) millis.milliseconds else null }

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
private fun cancelled(previous: InvokeOutcome?): InvokeOutcome.Interrupted { val values = when (previous) {
    is InvokeOutcome.Completed -> previous.items to previous.usage
    is InvokeOutcome.Interrupted -> previous.partialItems to previous.usage
    else -> emptyList<ModelItem>() to unknown() }
    return InvokeOutcome.Interrupted(InterruptionKind.CANCELLED, values.first, values.second) }
private fun AnthropicAdapterError.portFailure() = when (this) {
    AnthropicAdapterError.INVALID_REQUEST -> ModelPortFailure.INVALID_REQUEST
    AnthropicAdapterError.UNSUPPORTED_CAPABILITY -> ModelPortFailure.UNSUPPORTED_CAPABILITY
    AnthropicAdapterError.INVALID_JSON, AnthropicAdapterError.INVARIANT -> ModelPortFailure.ADAPTER_INVARIANT }

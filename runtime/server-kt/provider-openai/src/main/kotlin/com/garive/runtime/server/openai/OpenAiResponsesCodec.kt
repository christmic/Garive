package com.garive.runtime.server.openai

import com.garive.runtime.server.llm.*
import java.time.Instant
import java.time.format.DateTimeFormatter
import kotlinx.serialization.json.*
import kotlin.time.Duration
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.Duration.Companion.seconds

enum class OpenAiAdapterError { INVALID_REQUEST, UNSUPPORTED_CAPABILITY, INVALID_JSON, INVARIANT }
sealed interface OpenAiResult<out T> {
    data class Success<T>(val value: T) : OpenAiResult<T>
    data class Failure(val error: OpenAiAdapterError) : OpenAiResult<Nothing>
}
sealed interface HttpErrorAction {
    data class Retry(val retryAfter: Duration?) : HttpErrorAction
    data class Terminal(val outcome: InvokeOutcome) : HttpErrorAction
}
data class HttpRequestDescriptor(val method: String, val path: String,
    val headers: List<Pair<String, String>>, val body: ByteArray)
data class HttpResponseDescriptor(val status: Int, val retryAfter: String?, val body: ByteArray)
enum class TransportFailure { CONNECTION, TIMEOUT }
sealed interface TransportResult {
    data class Success(val response: HttpResponseDescriptor) : TransportResult
    data class Failure(val reason: TransportFailure) : TransportResult
}
interface OpenAiTransport {
    suspend fun execute(request: HttpRequestDescriptor, cancellation: ModelCancellation): TransportResult
    suspend fun wait(delay: Duration)
}

class OpenAiModelPort(private val transport: OpenAiTransport, private val maxAttempts: Int) : ModelPort {
    override suspend fun invoke(request: ModelRequest, observer: ModelObserver,
        cancellation: ModelCancellation): ModelPortResult {
        if (maxAttempts <= 0) return ModelPortResult.Failure(ModelPortFailure.INVALID_REQUEST)
        if (cancellation.isCancelled()) return ModelPortResult.Success(cancelled(null))
        for (attempt in 1..maxAttempts) {
            val descriptor = when (val rendered = OpenAiResponsesCodec.renderHttpRequest(request, true)) {
                is OpenAiResult.Success -> rendered.value
                is OpenAiResult.Failure -> return ModelPortResult.Failure(rendered.error.portFailure())
            }
            val response = transport.execute(descriptor, cancellation)
            if (cancellation.isCancelled()) return ModelPortResult.Success(cancelled(null))
            val wire = when (response) {
                is TransportResult.Success -> response.response
                is TransportResult.Failure -> if (attempt < maxAttempts) {
                    transport.wait(Duration.ZERO); continue
                } else return ModelPortResult.Success(InvokeOutcome.Interrupted(
                    InterruptionKind.TRANSPORT, emptyList(), unknownUsage()))
            }
            if (wire.status in 200..299) {
                val outcome = when (val parsed = OpenAiResponsesCodec.parseSse(wire.body)) {
                    is OpenAiResult.Success -> parsed.value
                    is OpenAiResult.Failure -> return ModelPortResult.Failure(parsed.error.portFailure())
                }
                return ModelPortResult.Success(if (cancellation.isCancelled()) cancelled(outcome) else outcome)
            }
            when (val action = OpenAiResponsesCodec.classifyHttpError(wire.status, wire.retryAfter,
                wire.body, attempt == maxAttempts, Instant.now())) {
                is OpenAiResult.Failure -> return ModelPortResult.Failure(action.error.portFailure())
                is OpenAiResult.Success -> when (val value = action.value) {
                    is HttpErrorAction.Retry -> transport.wait(value.retryAfter ?: Duration.ZERO)
                    is HttpErrorAction.Terminal -> return ModelPortResult.Success(value.outcome)
                }
            }
        }
        return ModelPortResult.Failure(ModelPortFailure.ADAPTER_INVARIANT)
    }
}
private enum class StreamKind { OUTPUT_TEXT, REFUSAL, FUNCTION_ARGUMENTS, REASONING_SUMMARY, REASONING_TEXT }
private data class StreamField(val kind: StreamKind, val subindex: UInt = 0u)
private data class StartedItem(val id: String, val kind: String, val callId: String?, val name: String?)

object OpenAiResponsesCodec {
    fun renderHttpRequest(request: ModelRequest, stream: Boolean): OpenAiResult<HttpRequestDescriptor> = guard {
        val body = when (val rendered = renderRequest(request, stream)) {
            is OpenAiResult.Success -> rendered.value.toString().encodeToByteArray()
            is OpenAiResult.Failure -> fail(rendered.error)
        }
        HttpRequestDescriptor("POST", "/v1/responses", listOf("content-type" to "application/json",
            "accept" to if (stream) "text/event-stream" else "application/json"), body)
    }

    fun classifyHttpError(
        status: Int,
        retryAfter: String?,
        body: ByteArray,
        exhausted: Boolean,
        now: Instant,
    ): OpenAiResult<HttpErrorAction> = guard {
        val error = parse(body.decodeToString()).jsonObject.getValue("error").jsonObject
        val code = error["code"]?.jsonPrimitive?.contentOrNull.orEmpty()
        val type = error["type"]?.jsonPrimitive?.contentOrNull.orEmpty()
        val evidence = "$type:$code".take(128)
        if (code == "context_length_exceeded") {
            return@guard HttpErrorAction.Terminal(InvokeOutcome.Rejected(RejectionKind.CONTEXT_OVERFLOW, evidence))
        }
        if (status == 401 || status == 403 || code == "invalid_api_key") {
            return@guard HttpErrorAction.Terminal(InvokeOutcome.Rejected(RejectionKind.AUTHENTICATION, evidence))
        }
        val kind = when (status) {
            429 -> UnavailableKind.RATE_LIMITED
            in 500..599 -> UnavailableKind.MODEL_UNAVAILABLE
            else -> fail(OpenAiAdapterError.UNSUPPORTED_CAPABILITY)
        }
        val delay = retryAfter?.let { parseRetryAfter(it, now) }
        if (!exhausted) HttpErrorAction.Retry(delay)
        else HttpErrorAction.Terminal(InvokeOutcome.Unavailable(kind, delay))
    }

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
        val assembled = mutableMapOf<Pair<UInt, StreamField>, StringBuilder>()
        val started = sortedMapOf<UInt, StartedItem>()
        val completed = sortedSetOf<UInt>()
        var terminal: InvokeOutcome? = null
        bytes.decodeToString().lineSequence().filter { it.startsWith("data: ") }.forEach { line ->
            if (terminal != null) fail(OpenAiAdapterError.INVARIANT)
            val event = parse(line.removePrefix("data: ")).jsonObject
            val sequence = event.ulong("sequence_number")
            if (previous?.let { sequence <= it } == true) fail(OpenAiAdapterError.INVARIANT)
            previous = sequence
            when (event.text("type")) {
                "response.output_item.added" -> {
                    val index = event.uint("output_index"); val item = event.getValue("item").jsonObject
                    val state = StartedItem(item.text("id"), item.text("type"), item["call_id"]?.jsonPrimitive?.contentOrNull,
                        item["name"]?.jsonPrimitive?.contentOrNull)
                    if (started.put(index, state) != null) fail(OpenAiAdapterError.INVARIANT)
                }
                "response.output_text.delta", "response.refusal.delta", "response.function_call_arguments.delta",
                "response.reasoning_summary_text.delta", "response.reasoning_text.delta" -> {
                    val index = event.uint("output_index"); val field = streamField(event)
                    requireStarted(started, completed, index, event, field)
                    assembled.getOrPut(index to field) { StringBuilder() }.append(event.text("delta"))
                }
                "response.output_text.done", "response.refusal.done", "response.function_call_arguments.done",
                "response.reasoning_summary_text.done", "response.reasoning_text.done" -> {
                    val index = event.uint("output_index"); val field = streamField(event)
                    requireStarted(started, completed, index, event, field)
                    val final = when (field.kind) { StreamKind.REFUSAL -> event.text("refusal")
                        StreamKind.FUNCTION_ARGUMENTS -> event.text("arguments"); else -> event.text("text") }
                    if (assembled[index to field]?.toString() != final) fail(OpenAiAdapterError.INVARIANT)
                }
                "response.output_item.done" -> {
                    val index = event.uint("output_index"); val item = event.getValue("item").jsonObject
                    val state = started[index] ?: fail(OpenAiAdapterError.INVARIANT)
                    if (state.id != item.text("id") || state.kind != item.text("type") || !completed.add(index))
                        fail(OpenAiAdapterError.INVARIANT)
                    verifyItemAssembled(assembled, index, item)
                }
                "response.content_part.added" -> verifyPartEvent(started, event, false)
                "response.content_part.done" -> { verifyPartEvent(started, event, true); verifyPartDone(assembled, event) }
                "response.reasoning_summary_part.added" -> verifySummaryPartEvent(started, event)
                "response.reasoning_summary_part.done" -> { verifySummaryPartEvent(started, event); verifyPartDone(assembled, event) }
                "response.completed" -> {
                    if (started.keys != completed) fail(OpenAiAdapterError.INVARIANT)
                    verifyAssembled(assembled, event.getValue("response").jsonObject)
                    terminal = response(event.getValue("response"))
                }
                "response.incomplete" -> {
                    val responseValue = event.getValue("response").jsonObject
                    verifyAssembled(assembled, responseValue)
                    terminal = response(responseValue)
                }
                "response.failed" -> fail(OpenAiAdapterError.INVARIANT)
                "response.created", "response.in_progress", "response.queued",
                "response.output_text.annotation.added" -> Unit
                else -> fail(OpenAiAdapterError.UNSUPPORTED_CAPABILITY)
            }
        }
        terminal ?: InvokeOutcome.Interrupted(InterruptionKind.TRANSPORT,
            assembledItems(assembled, started), unknownUsage())
    }

    private fun response(element: JsonElement): InvokeOutcome {
        val value = element.jsonObject
        val status = value.text("status")
        if (status != "completed" && status != "incomplete") fail(OpenAiAdapterError.INVARIANT)
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
                "reasoning" -> {
                    (item["summary"] as? JsonArray)?.forEach { items += ModelItem.Reasoning(
                        ReasoningContent.ModelVisible(it.jsonObject.text("text"))) }
                    (item["content"] as? JsonArray)?.forEach { items += ModelItem.Reasoning(
                        ReasoningContent.ModelVisible(it.jsonObject.text("text"))) }
                    item["encrypted_content"]?.jsonPrimitive?.contentOrNull?.let {
                        items += ModelItem.Reasoning(ReasoningContent.OpaqueReference(it))
                    }
                }
                else -> fail(OpenAiAdapterError.UNSUPPORTED_CAPABILITY)
            }
        }
        val usage = usage(value.getValue("usage").jsonObject)
        val stop = when { items.any { it is ModelItem.ToolIntent } -> ModelStopReason.ToolUse
            items.any { it is ModelItem.Refusal } -> ModelStopReason.Refusal else -> ModelStopReason.EndTurn }
        if (status == "incomplete") {
            when (value.getValue("incomplete_details").jsonObject.text("reason")) {
                "max_output_tokens" -> Unit
                "content_filter" -> return InvokeOutcome.Rejected(RejectionKind.CONTENT_POLICY, "incomplete:content_filter")
                else -> fail(OpenAiAdapterError.UNSUPPORTED_CAPABILITY)
            }
            return InvokeOutcome.Interrupted(InterruptionKind.OUTPUT_LIMIT, items, usage)
        }
        return InvokeOutcome.Completed(items, usage, stop)
    }

    private fun verifyAssembled(assembled: Map<Pair<UInt, StreamField>, StringBuilder>, response: JsonObject) {
        val output = response.getValue("output").jsonArray
        assembled.forEach { (key, text) -> val (index, field) = key
            val item = output.getOrNull(index.toInt())?.jsonObject ?: fail(OpenAiAdapterError.INVARIANT)
            val final = itemFieldText(item, field)
            if (text.toString() != final) fail(OpenAiAdapterError.INVARIANT)
        }
    }
    private fun verifyItemAssembled(values: Map<Pair<UInt, StreamField>, StringBuilder>, index: UInt, item: JsonObject) {
        values.filterKeys { it.first == index }.forEach { (key, text) ->
            if (text.toString() != itemFieldText(item, key.second)) fail(OpenAiAdapterError.INVARIANT) }
    }
    private fun itemFieldText(item: JsonObject, field: StreamField) = when (field.kind) {
        StreamKind.OUTPUT_TEXT -> indexedText(item, "content", field.subindex, "text")
        StreamKind.REFUSAL -> indexedText(item, "content", field.subindex, "refusal")
        StreamKind.FUNCTION_ARGUMENTS -> item.text("arguments")
        StreamKind.REASONING_SUMMARY -> indexedText(item, "summary", field.subindex, "text")
        StreamKind.REASONING_TEXT -> indexedText(item, "content", field.subindex, "text") }
    private fun verifyPartEvent(started: Map<UInt, StartedItem>, event: JsonObject, done: Boolean) {
        val state = started[event.uint("output_index")] ?: fail(OpenAiAdapterError.INVARIANT)
        if (state.id != event.text("item_id") || state.kind != "message") fail(OpenAiAdapterError.INVARIANT)
        val part = event.getValue("part").jsonObject
        if (part.text("type") !in setOf("output_text", "refusal", "reasoning_text")) fail(OpenAiAdapterError.UNSUPPORTED_CAPABILITY)
        if (done && event["part"] !is JsonObject) fail(OpenAiAdapterError.INVARIANT)
    }
    private fun verifySummaryPartEvent(started: Map<UInt, StartedItem>, event: JsonObject) {
        val state = started[event.uint("output_index")] ?: fail(OpenAiAdapterError.INVARIANT)
        if (state.id != event.text("item_id") || state.kind != "reasoning" ||
            event.getValue("part").jsonObject.text("type") != "summary_text") fail(OpenAiAdapterError.INVARIANT)
    }
    private fun verifyPartDone(values: Map<Pair<UInt, StreamField>, StringBuilder>, event: JsonObject) {
        val part = event.getValue("part").jsonObject
        val field = when (part.text("type")) { "output_text" -> StreamField(StreamKind.OUTPUT_TEXT, event.uint("content_index"))
            "refusal" -> StreamField(StreamKind.REFUSAL, event.uint("content_index"))
            "reasoning_text" -> StreamField(StreamKind.REASONING_TEXT, event.uint("content_index"))
            "summary_text" -> StreamField(StreamKind.REASONING_SUMMARY, event.uint("summary_index"))
            else -> fail(OpenAiAdapterError.UNSUPPORTED_CAPABILITY) }
        values[event.uint("output_index") to field]?.let { value ->
            val key = if (field.kind == StreamKind.REFUSAL) "refusal" else "text"
            if (part.text(key) != value.toString()) fail(OpenAiAdapterError.INVARIANT) }
    }

    private fun streamField(event: JsonObject): StreamField = when (event.text("type")) {
        "response.output_text.delta", "response.output_text.done" -> StreamField(StreamKind.OUTPUT_TEXT, event.uint("content_index"))
        "response.refusal.delta", "response.refusal.done" -> StreamField(StreamKind.REFUSAL, event.uint("content_index"))
        "response.function_call_arguments.delta", "response.function_call_arguments.done" -> StreamField(StreamKind.FUNCTION_ARGUMENTS)
        "response.reasoning_summary_text.delta", "response.reasoning_summary_text.done" -> StreamField(StreamKind.REASONING_SUMMARY, event.uint("summary_index"))
        "response.reasoning_text.delta", "response.reasoning_text.done" -> StreamField(StreamKind.REASONING_TEXT, event.uint("content_index"))
        else -> fail(OpenAiAdapterError.INVARIANT)
    }
    private fun requireStarted(started: Map<UInt, StartedItem>, completed: Set<UInt>, index: UInt,
        event: JsonObject, field: StreamField) { val state = started[index] ?: fail(OpenAiAdapterError.INVARIANT)
        val expected = when (field.kind) { StreamKind.OUTPUT_TEXT, StreamKind.REFUSAL -> "message"
            StreamKind.FUNCTION_ARGUMENTS -> "function_call"; StreamKind.REASONING_SUMMARY, StreamKind.REASONING_TEXT -> "reasoning" }
        if (index in completed || state.kind != expected || state.id != event.text("item_id")) fail(OpenAiAdapterError.INVARIANT) }
    private fun assembledItems(values: Map<Pair<UInt, StreamField>, StringBuilder>, started: Map<UInt, StartedItem>) =
        values.entries.sortedWith(compareBy({ it.key.first }, { it.key.second.kind.ordinal }, { it.key.second.subindex }))
            .map { (key, text) -> val (index, field) = key; when (field.kind) {
                StreamKind.OUTPUT_TEXT -> ModelItem.Text(text.toString()); StreamKind.REFUSAL -> ModelItem.Refusal(text.toString())
                StreamKind.FUNCTION_ARGUMENTS -> ModelItem.ToolIntent(started[index]?.callId.orEmpty(), started[index]?.name.orEmpty(), text.toString())
                StreamKind.REASONING_SUMMARY, StreamKind.REASONING_TEXT -> ModelItem.Reasoning(ReasoningContent.ModelVisible(text.toString())) } }
    private fun indexedText(item: JsonObject, list: String, index: UInt, key: String) =
        item.getValue(list).jsonArray.getOrNull(index.toInt())?.jsonObject?.text(key) ?: fail(OpenAiAdapterError.INVARIANT)

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

private fun parseRetryAfter(value: String, now: Instant): Duration? {
    value.trim().toLongOrNull()?.let { if (it >= 0) return it.seconds }
    val deadline = runCatching { Instant.from(DateTimeFormatter.RFC_1123_DATE_TIME.parse(value)) }.getOrNull()
        ?: return null
    val millis = deadline.toEpochMilli() - now.toEpochMilli()
    return if (millis >= 0) millis.milliseconds else null
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
private fun cancelled(previous: InvokeOutcome?): InvokeOutcome.Interrupted { val values = when (previous) {
    is InvokeOutcome.Completed -> previous.items to previous.usage
    is InvokeOutcome.Interrupted -> previous.partialItems to previous.usage
    else -> emptyList<ModelItem>() to unknownUsage() }
    return InvokeOutcome.Interrupted(InterruptionKind.CANCELLED, values.first, values.second) }
private fun OpenAiAdapterError.portFailure() = when (this) {
    OpenAiAdapterError.INVALID_REQUEST -> ModelPortFailure.INVALID_REQUEST
    OpenAiAdapterError.UNSUPPORTED_CAPABILITY -> ModelPortFailure.UNSUPPORTED_CAPABILITY
    OpenAiAdapterError.INVALID_JSON, OpenAiAdapterError.INVARIANT -> ModelPortFailure.ADAPTER_INVARIANT }

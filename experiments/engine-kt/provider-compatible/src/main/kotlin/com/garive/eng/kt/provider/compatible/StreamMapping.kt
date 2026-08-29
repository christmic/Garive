package com.garive.eng.kt.provider.compatible

import com.garive.eng.kt.anthropic.DeltaKind as MessagesDeltaKind
import com.garive.eng.kt.anthropic.PortableEventKind as MessagesEventKind
import com.garive.eng.kt.anthropic.StopReason
import com.garive.eng.kt.anthropic.StreamEvent as MessagesEvent
import com.garive.eng.kt.llm.InterruptionKind
import com.garive.eng.kt.llm.InvokeOutcome
import com.garive.eng.kt.llm.ModelItem
import com.garive.eng.kt.llm.ModelOutputKind
import com.garive.eng.kt.llm.ModelStreamEvent
import com.garive.eng.kt.llm.ReasoningContent
import com.garive.eng.kt.llm.RejectionKind
import com.garive.eng.kt.openai.OutputContent as ResponsesContent
import com.garive.eng.kt.openai.PortableEventKind as ResponsesEventKind
import com.garive.eng.kt.openai.ResponseEnvelope
import com.garive.eng.kt.openai.ResponseOutputItem
import com.garive.eng.kt.openai.ResponseStreamEvent
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put

/** Semantic events and optional terminal produced by one protocol stream event. */
public data class StreamMapping(
    public val events: List<ModelStreamEvent> = emptyList(),
    public val terminal: InvokeOutcome? = null,
)

/** Stateful mapper for adapter-validated Responses stream events. */
public class ResponsesStreamMapper(private val reasoningVisibility: Boolean) {
    private data class Open(public val index: UInt, public val kind: ModelOutputKind)
    private val open: MutableMap<String, Open> = mutableMapOf()
    private var nextIndex: UInt = 0u

    /** Converts one protocol event into neutral semantic facts. */
    public fun accept(event: ResponseStreamEvent): StreamMapping {
        val portable = event as? ResponseStreamEvent.Portable ?: fail(CompatibleProviderError.UNSUPPORTED_EXTENSION)
        val raw = portable.raw
        return when (portable.kind) {
            ResponsesEventKind.CREATED, ResponsesEventKind.QUEUED, ResponsesEventKind.IN_PROGRESS,
            ResponsesEventKind.OUTPUT_TEXT_ANNOTATION_ADDED,
            -> StreamMapping()
            ResponsesEventKind.OUTPUT_ITEM_ADDED -> itemStart(raw)
            ResponsesEventKind.CONTENT_PART_ADDED -> contentStart(raw)
            ResponsesEventKind.OUTPUT_TEXT_DELTA -> delta(raw, true, Delta.TEXT)
            ResponsesEventKind.REFUSAL_DELTA -> delta(raw, true, Delta.REFUSAL)
            ResponsesEventKind.FUNCTION_ARGUMENTS_DELTA -> delta(raw, false, Delta.TOOL)
            ResponsesEventKind.REASONING_SUMMARY_TEXT_DELTA, ResponsesEventKind.REASONING_TEXT_DELTA -> {
                if (reasoningVisibility) delta(raw, false, Delta.REASONING) else StreamMapping()
            }
            ResponsesEventKind.CONTENT_PART_DONE -> contentDone(raw)
            ResponsesEventKind.OUTPUT_ITEM_DONE -> itemDone(raw)
            ResponsesEventKind.COMPLETED, ResponsesEventKind.INCOMPLETE -> StreamMapping(
                terminal = normalizeResponses(ResponseEnvelope.parse(raw.getValue("response").jsonObject), reasoningVisibility),
            )
            ResponsesEventKind.FAILED, ResponsesEventKind.ERROR -> fail(CompatibleProviderError.UNCLASSIFIED_PROTOCOL_ERROR)
            ResponsesEventKind.OUTPUT_TEXT_DONE, ResponsesEventKind.REFUSAL_DONE,
            ResponsesEventKind.FUNCTION_ARGUMENTS_DONE, ResponsesEventKind.REASONING_SUMMARY_PART_ADDED,
            ResponsesEventKind.REASONING_SUMMARY_PART_DONE, ResponsesEventKind.REASONING_SUMMARY_TEXT_DONE,
            ResponsesEventKind.REASONING_TEXT_DONE,
            -> StreamMapping()
        }
    }

    private fun itemStart(raw: JsonObject): StreamMapping {
        val outputIndex = raw.ulong("output_index")
        val item = parseResponseItem(raw.getValue("item").jsonObject)
        val kind = when (item) {
            is ResponseOutputItem.FunctionCall -> ModelOutputKind.ToolIntent(item.callId)
            is ResponseOutputItem.Reasoning -> ModelOutputKind.Reasoning
            is ResponseOutputItem.Message -> return StreamMapping()
            is ResponseOutputItem.Extension -> fail(CompatibleProviderError.UNSUPPORTED_EXTENSION)
        }
        return start("item:$outputIndex", kind)
    }

    private fun contentStart(raw: JsonObject): StreamMapping {
        val part = parseResponseContent(raw.getValue("part").jsonObject)
        val kind = when (part) {
            is ResponsesContent.Text -> ModelOutputKind.Text
            is ResponsesContent.Refusal -> ModelOutputKind.Refusal
            is ResponsesContent.Extension -> fail(CompatibleProviderError.UNSUPPORTED_EXTENSION)
        }
        return start(contentKey(raw), kind)
    }

    private fun start(key: String, kind: ModelOutputKind): StreamMapping {
        val index = nextIndex
        if (nextIndex == UInt.MAX_VALUE) fail(CompatibleProviderError.PROTOCOL_INVARIANT)
        nextIndex++
        if (open.put(key, Open(index, kind)) != null) fail(CompatibleProviderError.PROTOCOL_INVARIANT)
        return StreamMapping(listOf(ModelStreamEvent.OutputItemStarted(index, kind)))
    }

    private fun delta(raw: JsonObject, content: Boolean, kind: Delta): StreamMapping {
        val output = raw.ulong("output_index")
        val key = if (content) "content:$output:${raw.ulong("content_index")}" else "item:$output"
        val state = open[key] ?: fail(CompatibleProviderError.PROTOCOL_INVARIANT)
        val value = raw.getValue("delta").jsonPrimitive.content
        val event = when (kind) {
            Delta.TEXT -> ModelStreamEvent.TextDelta(state.index, value)
            Delta.REFUSAL -> ModelStreamEvent.RefusalDelta(state.index, value)
            Delta.REASONING -> ModelStreamEvent.ReasoningDelta(state.index, value)
            Delta.TOOL -> {
                val tool = state.kind as? ModelOutputKind.ToolIntent ?: fail(CompatibleProviderError.PROTOCOL_INVARIANT)
                ModelStreamEvent.ToolArgumentsDelta(state.index, tool.modelCallId, value)
            }
        }
        return StreamMapping(listOf(event))
    }

    private fun contentDone(raw: JsonObject): StreamMapping {
        val state = open.remove(contentKey(raw)) ?: fail(CompatibleProviderError.PROTOCOL_INVARIANT)
        val item = when (val part = parseResponseContent(raw.getValue("part").jsonObject)) {
            is ResponsesContent.Text -> ModelItem.Text(part.text)
            is ResponsesContent.Refusal -> ModelItem.Refusal(part.refusal)
            is ResponsesContent.Extension -> fail(CompatibleProviderError.UNSUPPORTED_EXTENSION)
        }
        return StreamMapping(listOf(ModelStreamEvent.OutputItemCompleted(state.index, item)))
    }

    private fun itemDone(raw: JsonObject): StreamMapping {
        val state = open.remove("item:${raw.ulong("output_index")}") ?: return StreamMapping()
        val items = responsesItems(listOf(parseResponseItem(raw.getValue("item").jsonObject)), reasoningVisibility)
        return StreamMapping(listOf(ModelStreamEvent.OutputItemCompleted(
            state.index,
            items.singleOrNull() ?: fail(CompatibleProviderError.PROTOCOL_INVARIANT),
        )))
    }

    private fun contentKey(raw: JsonObject): String =
        "content:${raw.ulong("output_index")}:${raw.ulong("content_index")}"

    private enum class Delta { TEXT, REFUSAL, REASONING, TOOL }
}

private sealed interface MessagesBlock {
    public data class Text(public val value: StringBuilder) : MessagesBlock
    public data class Tool(
        public val id: String,
        public val name: String,
        public val arguments: StringBuilder,
    ) : MessagesBlock
    public data class Thinking(public val text: StringBuilder, public val signature: StringBuilder) : MessagesBlock
    public data class Redacted(public val data: String) : MessagesBlock
}

/** Stateful mapper for adapter-validated Messages stream events. */
public class MessagesStreamMapper(private val reasoningVisibility: Boolean) {
    private val open: MutableMap<UInt, MessagesBlock> = mutableMapOf()
    private val items: MutableList<ModelItem> = mutableListOf()
    private var inputTokens: ULong? = null
    private var cacheReadTokens: ULong? = null
    private var cacheWriteTokens: ULong? = null
    private var outputTokens: ULong? = null
    private var stopReason: StopReason? = null

    /** Converts one protocol event into neutral semantic facts. */
    public fun accept(event: MessagesEvent): StreamMapping {
        val portable = event as? MessagesEvent.Portable ?: fail(CompatibleProviderError.UNSUPPORTED_EXTENSION)
        return when (portable.kind) {
            MessagesEventKind.MESSAGE_START -> {
                val usage = portable.raw.getValue("message").jsonObject.getValue("usage").jsonObject
                inputTokens = usage.optionalULong("input_tokens")
                cacheReadTokens = usage.optionalULong("cache_read_input_tokens")
                cacheWriteTokens = usage.optionalULong("cache_creation_input_tokens")
                StreamMapping()
            }
            MessagesEventKind.CONTENT_BLOCK_START -> start(portable.raw)
            MessagesEventKind.CONTENT_BLOCK_DELTA -> delta(
                portable.raw,
                portable.deltaKind ?: fail(CompatibleProviderError.UNSUPPORTED_EXTENSION),
            )
            MessagesEventKind.CONTENT_BLOCK_STOP -> stop(portable.raw)
            MessagesEventKind.MESSAGE_DELTA -> {
                outputTokens = portable.raw.getValue("usage").jsonObject.optionalULong("output_tokens")
                stopReason = portable.raw.getValue("delta").jsonObject["stop_reason"]
                    ?.takeUnless { it is JsonNull }?.jsonPrimitive?.content
                    ?.let { value -> StopReason.entries.firstOrNull { it.name.lowercase() == value } }
                StreamMapping(listOf(ModelStreamEvent.UsageUpdated(streamUsage())))
            }
            MessagesEventKind.MESSAGE_STOP -> terminal()
            MessagesEventKind.PING -> StreamMapping()
            MessagesEventKind.ERROR -> fail(CompatibleProviderError.UNCLASSIFIED_PROTOCOL_ERROR)
        }
    }

    private fun start(raw: JsonObject): StreamMapping {
        val index = raw.uint("index")
        val block = raw.getValue("content_block").jsonObject
        val state: MessagesBlock
        val kind: ModelOutputKind
        when (block.getValue("type").jsonPrimitive.content) {
            "text" -> {
                state = MessagesBlock.Text(StringBuilder(block["text"]?.jsonPrimitive?.content.orEmpty()))
                kind = ModelOutputKind.Text
            }
            "tool_use" -> {
                val id = block.getValue("id").jsonPrimitive.content
                state = MessagesBlock.Tool(id, block.getValue("name").jsonPrimitive.content, StringBuilder())
                kind = ModelOutputKind.ToolIntent(id)
            }
            "thinking" -> {
                state = MessagesBlock.Thinking(
                    StringBuilder(block["thinking"]?.jsonPrimitive?.content.orEmpty()),
                    StringBuilder(block["signature"]?.jsonPrimitive?.content.orEmpty()),
                )
                kind = ModelOutputKind.Reasoning
            }
            "redacted_thinking" -> {
                state = MessagesBlock.Redacted(block.getValue("data").jsonPrimitive.content)
                kind = ModelOutputKind.Reasoning
            }
            else -> fail(CompatibleProviderError.UNSUPPORTED_EXTENSION)
        }
        if (open.put(index, state) != null) fail(CompatibleProviderError.PROTOCOL_INVARIANT)
        return StreamMapping(listOf(ModelStreamEvent.OutputItemStarted(index, kind)))
    }

    private fun delta(raw: JsonObject, kind: MessagesDeltaKind): StreamMapping {
        val index = raw.uint("index")
        val block = open[index] ?: fail(CompatibleProviderError.PROTOCOL_INVARIANT)
        val delta = raw.getValue("delta").jsonObject
        val event: ModelStreamEvent? = when {
            block is MessagesBlock.Text && kind == MessagesDeltaKind.TEXT -> {
                val value = delta.getValue("text").jsonPrimitive.content
                block.value.append(value)
                ModelStreamEvent.TextDelta(index, value)
            }
            block is MessagesBlock.Tool && kind == MessagesDeltaKind.INPUT_JSON -> {
                val value = delta.getValue("partial_json").jsonPrimitive.content
                block.arguments.append(value)
                ModelStreamEvent.ToolArgumentsDelta(index, block.id, value)
            }
            block is MessagesBlock.Thinking && kind == MessagesDeltaKind.THINKING -> {
                val value = delta.getValue("thinking").jsonPrimitive.content
                block.text.append(value)
                if (reasoningVisibility) ModelStreamEvent.ReasoningDelta(index, value) else null
            }
            block is MessagesBlock.Thinking && kind == MessagesDeltaKind.SIGNATURE -> {
                block.signature.append(delta.getValue("signature").jsonPrimitive.content)
                null
            }
            kind == MessagesDeltaKind.CITATION -> null
            else -> fail(CompatibleProviderError.PROTOCOL_INVARIANT)
        }
        return StreamMapping(event?.let(::listOf).orEmpty())
    }

    private fun stop(raw: JsonObject): StreamMapping {
        val index = raw.uint("index")
        val item = when (val block = open.remove(index) ?: fail(CompatibleProviderError.PROTOCOL_INVARIANT)) {
            is MessagesBlock.Text -> ModelItem.Text(block.value.toString())
            is MessagesBlock.Tool -> ModelItem.ToolIntent(
                block.id,
                block.name,
                canonicalStreamObject(block.arguments.toString()),
            )
            is MessagesBlock.Thinking -> ModelItem.Reasoning(
                if (reasoningVisibility) ReasoningContent.ModelVisible(block.text.toString())
                else ReasoningContent.OpaqueReference(block.signature.toString()),
            )
            is MessagesBlock.Redacted -> ModelItem.Reasoning(ReasoningContent.OpaqueReference(block.data))
        }
        items += item
        return StreamMapping(listOf(ModelStreamEvent.OutputItemCompleted(index, item)))
    }

    private fun terminal(): StreamMapping {
        val reason = stopReason ?: fail(CompatibleProviderError.PROTOCOL_INVARIANT)
        val finalItems = if (reason == StopReason.REFUSAL) items.map {
            if (it is ModelItem.Text) ModelItem.Refusal(it.text) else it
        } else items.toList()
        val outcome = when (reason) {
            StopReason.MAX_TOKENS -> InvokeOutcome.Interrupted(InterruptionKind.OUTPUT_LIMIT, finalItems, streamUsage())
            StopReason.MODEL_CONTEXT_WINDOW_EXCEEDED -> InvokeOutcome.Rejected(
                RejectionKind.CONTEXT_OVERFLOW,
                "model_context_window_exceeded",
            )
            else -> InvokeOutcome.Completed(finalItems, streamUsage(), messagesStop(reason))
        }
        return StreamMapping(terminal = outcome)
    }

    private fun streamUsage(): com.garive.eng.kt.llm.ModelUsage = com.garive.eng.kt.llm.ModelUsage(
        inputTokens?.let(com.garive.eng.kt.llm.TokenCount::Known) ?: com.garive.eng.kt.llm.TokenCount.Unknown,
        outputTokens?.let(com.garive.eng.kt.llm.TokenCount::Known) ?: com.garive.eng.kt.llm.TokenCount.Unknown,
        cacheReadTokens?.let(com.garive.eng.kt.llm.TokenCount::Known),
        cacheWriteTokens?.let(com.garive.eng.kt.llm.TokenCount::Known),
        com.garive.eng.kt.llm.UsageSource.PROVIDER_REPORTED,
    )
}

private fun parseResponseItem(raw: JsonObject): ResponseOutputItem = parseResponse(listOf(raw)).output.single()

private fun parseResponseContent(raw: JsonObject): ResponsesContent {
    val message = buildJsonObject {
        put("type", "message"); put("id", "stream-item"); put("role", "assistant"); put("status", "completed")
        put("content", JsonArray(listOf(raw)))
    }
    return (parseResponse(listOf(message)).output.single() as ResponseOutputItem.Message).content.single()
}

private fun parseResponse(output: List<JsonObject>): ResponseEnvelope = ResponseEnvelope.parse(buildJsonObject {
    put("id", "stream-response"); put("created_at", 0.0); put("model", "stream-model"); put("object", "response")
    put("status", "completed"); put("error", JsonNull); put("incomplete_details", JsonNull)
    put("instructions", JsonNull); put("metadata", JsonNull); put("output", JsonArray(output))
    put("parallel_tool_calls", false); put("tool_choice", "auto"); put("tools", JsonArray(emptyList()))
    put("temperature", JsonNull); put("top_p", JsonNull); put("usage", JsonNull)
})

private fun canonicalStreamObject(value: String): String = try {
    (Json.parseToJsonElement(value) as? JsonObject)?.toString()
        ?: fail(CompatibleProviderError.PROTOCOL_INVARIANT)
} catch (error: CompatibleProviderException) {
    throw error
} catch (_: IllegalArgumentException) {
    fail(CompatibleProviderError.PROTOCOL_INVARIANT)
}

private fun JsonObject.ulong(name: String): ULong = getValue(name).jsonPrimitive.content.toULong()
private fun JsonObject.uint(name: String): UInt = getValue(name).jsonPrimitive.content.toUInt()
private fun JsonObject.optionalULong(name: String): ULong? = get(name)?.takeUnless { it is JsonNull }?.jsonPrimitive?.content?.toULong()

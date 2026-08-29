package com.garive.eng.kt.openai

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject

private data class ItemState(
    val id: String,
    val kind: String,
    var done: Boolean = false,
    val content: MutableSet<ULong> = mutableSetOf(),
    val contentDone: MutableSet<ULong> = mutableSetOf(),
)

/** Incremental typed Responses stream decoder with item/content lifecycle validation. */
public class ResponsesStreamDecoder {
    private val sse: SseDecoder = SseDecoder()
    private var previous: ULong? = null
    private var created: Boolean = false
    private var terminal: Boolean = false
    private var sentinel: Boolean = false
    private val items: MutableMap<ULong, ItemState> = mutableMapOf()

    /** Appends bytes and emits every complete validated protocol event immediately. */
    public fun push(bytes: ByteArray): List<ResponseStreamEvent> = responseFailure(ResponsesProtocolError.INVALID_LIFECYCLE) {
        val events = mutableListOf<ResponseStreamEvent>()
        sse.push(bytes).forEach { frame ->
            if (frame.data == "[DONE]") {
                require(terminal && !sentinel && frame.event == null)
                sentinel = true
            } else {
                require(!sentinel)
                val value = RESPONSES_JSON.parseToJsonElement(frame.data).jsonObject
                val event = ResponseStreamEvent.parse(value)
                require(frame.event == null || frame.event == event.discriminator)
                accept(event)
                events += event
            }
        }
        return events
    }

    /** Requires one protocol terminal and complete SSE framing at EOF. */
    public fun finish(): Unit = responseFailure(ResponsesProtocolError.TRUNCATED_STREAM) {
        sse.finish()
        require(terminal)
    }

    private fun accept(event: ResponseStreamEvent): Unit {
        require(!terminal)
        event.sequenceNumber?.let { sequence ->
            require(previous == null || sequence > requireNotNull(previous))
            previous = sequence
        }
        when (event) {
            is ResponseStreamEvent.Extension -> {
                require(created)
                return
            }
            is ResponseStreamEvent.Portable -> acceptPortable(event)
        }
    }

    private fun acceptPortable(event: ResponseStreamEvent.Portable): Unit {
        val value = event.raw
        when (event.kind) {
            PortableEventKind.CREATED -> {
                require(!created && previous == event.sequenceNumber)
                created = true
            }
            else -> require(created)
        }
        when (event.kind) {
            PortableEventKind.COMPLETED -> {
                require(items.values.none { !it.done })
                terminal = true
            }
            PortableEventKind.FAILED,
            PortableEventKind.INCOMPLETE,
            PortableEventKind.ERROR,
            -> terminal = true

            PortableEventKind.OUTPUT_ITEM_ADDED -> addItem(value)
            PortableEventKind.OUTPUT_ITEM_DONE -> finishItem(value)
            PortableEventKind.CONTENT_PART_ADDED -> addContent(value)
            PortableEventKind.CONTENT_PART_DONE -> finishContent(value)
            PortableEventKind.OUTPUT_TEXT_DELTA,
            PortableEventKind.OUTPUT_TEXT_DONE,
            PortableEventKind.REFUSAL_DELTA,
            PortableEventKind.REFUSAL_DONE,
            PortableEventKind.OUTPUT_TEXT_ANNOTATION_ADDED,
            -> requireContent(value)

            PortableEventKind.FUNCTION_ARGUMENTS_DELTA,
            PortableEventKind.FUNCTION_ARGUMENTS_DONE,
            -> requireItemKind(value, ResponseKinds.FUNCTION_CALL)

            PortableEventKind.REASONING_SUMMARY_PART_ADDED,
            PortableEventKind.REASONING_SUMMARY_PART_DONE,
            PortableEventKind.REASONING_SUMMARY_TEXT_DELTA,
            PortableEventKind.REASONING_SUMMARY_TEXT_DONE,
            PortableEventKind.REASONING_TEXT_DELTA,
            PortableEventKind.REASONING_TEXT_DONE,
            -> requireItemKind(value, ResponseKinds.REASONING)

            PortableEventKind.CREATED,
            PortableEventKind.QUEUED,
            PortableEventKind.IN_PROGRESS,
            -> Unit
        }
    }

    private fun addItem(value: JsonObject): Unit {
        val index = value.requiredULong("output_index")
        val item = value.getValue("item").jsonObject
        val id = item.requiredText("id"); val kind = item.requiredText(ResponseFields.TYPE)
        require(index !in items && items.values.none { it.id == id })
        items[index] = ItemState(id, kind)
    }

    private fun finishItem(value: JsonObject): Unit {
        val index = value.requiredULong("output_index")
        val item = value.getValue("item").jsonObject
        val state = requireNotNull(items[index])
        require(!state.done && state.id == item.requiredText("id") && state.kind == item.requiredText(ResponseFields.TYPE))
        require(state.content == state.contentDone)
        state.done = true
    }

    private fun addContent(value: JsonObject): Unit {
        val state = itemFor(value)
        val contentIndex = value.requiredULong("content_index")
        require(state.kind == ResponseKinds.MESSAGE && !state.done && state.content.add(contentIndex))
    }

    private fun finishContent(value: JsonObject): Unit {
        val state = itemFor(value)
        val contentIndex = value.requiredULong("content_index")
        require(contentIndex in state.content && state.contentDone.add(contentIndex))
    }

    private fun requireContent(value: JsonObject): Unit {
        val state = itemFor(value)
        val index = value.requiredULong("content_index")
        require(!state.done && index in state.content && index !in state.contentDone)
    }

    private fun requireItemKind(value: JsonObject, kind: String): Unit {
        val state = itemFor(value)
        require(!state.done && state.kind == kind)
    }

    private fun itemFor(value: JsonObject): ItemState {
        val state = requireNotNull(items[value.requiredULong("output_index")])
        require(state.id == value.requiredText("item_id"))
        return state
    }
}

/** Creates a fresh decoder for one streaming exchange. */
public fun ResponsesAdapter.streamDecoder(): ResponsesStreamDecoder = ResponsesStreamDecoder()

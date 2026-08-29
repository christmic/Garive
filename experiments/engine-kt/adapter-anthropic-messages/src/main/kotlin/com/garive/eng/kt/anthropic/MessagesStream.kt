package com.garive.eng.kt.anthropic

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject

private data class OpenBlock(val kind: String, val partialJson: StringBuilder = StringBuilder())

/** Incremental Messages SSE decoder with block and terminal lifecycle validation. */
public class MessagesStreamDecoder {
    private val sse: SseDecoder = SseDecoder()
    private var started: Boolean = false
    private var terminal: Boolean = false
    private var messageDelta: Boolean = false
    private val blocks: MutableMap<UInt, OpenBlock> = mutableMapOf()

    /** Appends arbitrary transport bytes and emits complete validated events immediately. */
    public fun push(bytes: ByteArray): List<StreamEvent> = messageFailure(MessagesProtocolError.INVALID_LIFECYCLE) {
        sse.push(bytes).map { frame ->
            val raw = MESSAGES_JSON.parseToJsonElement(frame.data).jsonObject
            val event = StreamEvent.parse(raw)
            require(frame.event == null || frame.event == event.discriminator)
            accept(event)
            event
        }
    }

    /** Requires one terminal, no open blocks, and complete SSE framing at EOF. */
    public fun finish(): Unit = messageFailure(MessagesProtocolError.TRUNCATED_STREAM) {
        sse.finish()
        require(terminal && blocks.isEmpty())
    }

    private fun accept(event: StreamEvent): Unit {
        require(!terminal)
        when (event) {
            is StreamEvent.Extension -> {
                require(started)
                return
            }
            is StreamEvent.Portable -> acceptPortable(event)
        }
    }

    private fun acceptPortable(event: StreamEvent.Portable): Unit {
        when (event.kind) {
            PortableEventKind.PING -> return
            PortableEventKind.MESSAGE_START -> {
                require(!started)
                started = true
            }
            PortableEventKind.ERROR -> terminal = true
            else -> {
                require(started)
                when (event.kind) {
                    PortableEventKind.CONTENT_BLOCK_START -> startBlock(event.raw)
                    PortableEventKind.CONTENT_BLOCK_DELTA -> deltaBlock(event.raw, event.deltaKind)
                    PortableEventKind.CONTENT_BLOCK_STOP -> stopBlock(event.raw)
                    PortableEventKind.MESSAGE_DELTA -> {
                        require(blocks.isEmpty() && !messageDelta)
                        messageDelta = true
                    }
                    PortableEventKind.MESSAGE_STOP -> {
                        require(blocks.isEmpty() && messageDelta)
                        terminal = true
                    }
                    PortableEventKind.MESSAGE_START,
                    PortableEventKind.PING,
                    PortableEventKind.ERROR,
                    -> error("handled above")
                }
            }
        }
    }

    private fun startBlock(value: JsonObject): Unit {
        val index = value.requiredUInt("index")
        val kind = value.getValue(MessageFields.CONTENT_BLOCK).jsonObject.requiredText(MessageFields.TYPE)
        require(blocks.put(index, OpenBlock(kind)) == null)
    }

    private fun deltaBlock(value: JsonObject, deltaKind: DeltaKind?): Unit {
        val index = value.requiredUInt("index")
        val block = requireNotNull(blocks[index])
        if (deltaKind != null) {
            require(
                when (block.kind) {
                    MessageKinds.TEXT -> deltaKind in setOf(DeltaKind.TEXT, DeltaKind.CITATION)
                    MessageKinds.TOOL_USE -> deltaKind == DeltaKind.INPUT_JSON
                    MessageKinds.THINKING -> deltaKind in setOf(DeltaKind.THINKING, DeltaKind.SIGNATURE)
                    else -> true
                },
            )
        }
        if (deltaKind == DeltaKind.INPUT_JSON) {
            block.partialJson.append(value.getValue(MessageFields.DELTA).jsonObject.requiredText(MessageFields.PARTIAL_JSON))
        }
    }

    private fun stopBlock(value: JsonObject): Unit {
        val block = requireNotNull(blocks.remove(value.requiredUInt("index")))
        if (block.kind == MessageKinds.TOOL_USE) MESSAGES_JSON.parseToJsonElement(block.partialJson.toString())
    }
}

/** Creates a fresh decoder for one streaming exchange. */
public fun MessagesAdapter.streamDecoder(): MessagesStreamDecoder = MessagesStreamDecoder()

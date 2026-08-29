package com.garive.eng.kt.provider.compatible

import com.garive.eng.kt.anthropic.MessageResponse
import com.garive.eng.kt.anthropic.MessagesStreamDecoder
import com.garive.eng.kt.llm.InvokeOutcome
import com.garive.eng.kt.llm.ModelStopReason
import com.garive.eng.kt.llm.StreamValidator
import com.garive.eng.kt.openai.ResponseEnvelope
import com.garive.eng.kt.openai.ResponsesStreamDecoder
import java.nio.file.Path
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs
import kotlin.test.assertNull
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

public class StreamMappingTest {
    @Test
    public fun `responses protocol stream maps valid neutral events and matching terminal`(): Unit {
        val added = message("in_progress", emptyList())
        val text = buildJsonObject { put("type", "output_text"); put("text", "hi"); put("annotations", JsonArray(emptyList())) }
        val done = message("completed", listOf(text))
        val values = listOf(
            event("response.created", 0u) { put("response", response("in_progress", emptyList())) },
            event("response.output_item.added", 1u) { put("output_index", 0); put("item", added) },
            event("response.content_part.added", 2u) {
                put("output_index", 0); put("content_index", 0); put("item_id", "msg")
                put("part", buildJsonObject { put("type", "output_text"); put("text", ""); put("annotations", JsonArray(emptyList())) })
            },
            event("response.output_text.delta", 3u) {
                put("output_index", 0); put("content_index", 0); put("item_id", "msg"); put("delta", "hi")
            },
            event("response.output_text.done", 4u) {
                put("output_index", 0); put("content_index", 0); put("item_id", "msg"); put("text", "hi")
            },
            event("response.content_part.done", 5u) {
                put("output_index", 0); put("content_index", 0); put("item_id", "msg"); put("part", text)
            },
            event("response.output_item.done", 6u) { put("output_index", 0); put("item", done) },
            event("response.completed", 7u) { put("response", response("completed", listOf(done))) },
        )
        val bytes = values.joinToString("") { "event: ${it["type"]!!.toString().trim('"')}\ndata: $it\n\n" }
            .plus("data: [DONE]\n\n").toByteArray()
        val decoder = ResponsesStreamDecoder()
        val protocol = decoder.push(bytes)
        decoder.finish()

        val mapper = ResponsesStreamMapper(false)
        val validator = StreamValidator()
        var terminal: InvokeOutcome? = null
        protocol.forEach { protocolEvent ->
            val mapping = mapper.accept(protocolEvent)
            mapping.events.forEach { assertNull(validator.accept(it)) }
            mapping.terminal?.let { terminal = it }
        }
        assertEquals(
            normalizeResponses(ResponseEnvelope.parse(response("completed", listOf(done))), false),
            terminal,
        )
    }

    @Test
    public fun `official messages stream maps valid neutral events and matching terminal`(): Unit {
        val bytes = Path.of(
            System.getProperty("garive.repo.root"),
            "spec/fixtures/protocols/anthropic-messages/complete.sse",
        ).toFile().readBytes()
        val decoder = MessagesStreamDecoder()
        val protocol = decoder.push(bytes)
        decoder.finish()
        val mapper = MessagesStreamMapper(false)
        val validator = StreamValidator()
        var terminal: InvokeOutcome? = null
        protocol.forEach { protocolEvent ->
            val mapping = mapper.accept(protocolEvent)
            mapping.events.forEach { assertNull(validator.accept(it)) }
            mapping.terminal?.let { terminal = it }
        }
        val buffered = MessageResponse.parse(buildJsonObject {
            put("id", "msg_stream"); put("type", "message"); put("role", "assistant"); put("model", "fixture")
            put("content", JsonArray(listOf(
                buildJsonObject { put("type", "text"); put("text", "hello back") },
                buildJsonObject {
                    put("type", "tool_use"); put("id", "toolu_1"); put("name", "weather")
                    put("input", buildJsonObject { put("city", "Paris") })
                },
            )))
            put("stop_reason", "tool_use"); put("stop_sequence", JsonNull)
            put("usage", buildJsonObject {
                put("input_tokens", 4); put("output_tokens", 5)
                put("cache_creation_input_tokens", 1); put("cache_read_input_tokens", 1)
            })
        })
        assertEquals(normalizeMessages(buffered, false), terminal)
        assertEquals(ModelStopReason.ToolUse, assertIs<InvokeOutcome.Completed>(terminal).stopReason)
    }

    private fun message(status: String, content: List<JsonObject>): JsonObject = buildJsonObject {
        put("id", "msg"); put("type", "message"); put("role", "assistant"); put("status", status)
        put("content", JsonArray(content))
    }

    private fun response(status: String, output: List<JsonObject>): JsonObject = buildJsonObject {
        put("id", "resp"); put("created_at", 1.0); put("error", JsonNull); put("incomplete_details", JsonNull)
        put("instructions", JsonNull); put("metadata", JsonNull); put("model", "model"); put("object", "response")
        put("output", JsonArray(output)); put("parallel_tool_calls", false); put("temperature", JsonNull)
        put("tool_choice", "auto"); put("tools", JsonArray(emptyList())); put("top_p", JsonNull)
        put("status", status); put("usage", JsonNull)
    }

    private fun event(type: String, sequence: ULong, content: JsonObjectBuilderScope.() -> Unit): JsonObject =
        buildJsonObject {
            put("type", type); put("sequence_number", sequence.toLong()); JsonObjectBuilderScope(this).content()
        }

    private class JsonObjectBuilderScope(private val builder: kotlinx.serialization.json.JsonObjectBuilder) {
        public fun put(name: String, value: Int): Unit { builder.put(name, value) }
        public fun put(name: String, value: JsonObject): Unit { builder.put(name, value) }
        public fun put(name: String, value: String): Unit { builder.put(name, value) }
    }
}

package com.garive.eng.kt.openai

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFails
import kotlin.test.assertIs
import kotlin.test.assertTrue
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.put

class ResponsesEventsTest {
    @Test
    fun `portable event catalogue uses exact official discriminators`() {
        assertEquals(24, PortableEventKind.entries.size)
        assertEquals("response.created", PortableEventKind.CREATED.wireName)
        assertEquals("response.function_call_arguments.delta", PortableEventKind.FUNCTION_ARGUMENTS_DELTA.wireName)
        assertEquals("error", PortableEventKind.ERROR.wireName)
        assertEquals(PortableEventKind.OUTPUT_TEXT_DONE, PortableEventKind.fromWireName("response.output_text.done"))
    }

    @Test
    fun `every portable non-root payload family validates`() {
        val values = listOf(
            """{"type":"response.output_text.delta","sequence_number":1,"output_index":0,"content_index":0,"item_id":"msg","delta":"你"}""",
            """{"type":"response.refusal.done","sequence_number":2,"output_index":0,"content_index":0,"item_id":"msg","refusal":"no"}""",
            """{"type":"response.function_call_arguments.done","sequence_number":3,"output_index":1,"item_id":"call","arguments":"{}"}""",
            """{"type":"response.reasoning_summary_text.delta","sequence_number":4,"output_index":2,"summary_index":0,"item_id":"reason","delta":"summary"}""",
            """{"type":"response.reasoning_text.done","sequence_number":5,"output_index":2,"content_index":0,"item_id":"reason","text":"detail"}""",
            """{"type":"response.output_text.annotation.added","sequence_number":6,"output_index":0,"content_index":0,"annotation_index":0,"item_id":"msg","annotation":{"type":"url_citation"}}""",
            """{"type":"error","sequence_number":7,"message":"bad","code":"future"}""",
        )
        values.forEach { encoded -> assertIs<ResponseStreamEvent.Portable>(ResponseStreamEvent.parse(json(encoded))) }
    }

    @Test
    fun `hosted event is a lossless extension`() {
        val raw = json("""{"type":"response.web_search_call.searching","sequence_number":7,"output_index":0,"item_id":"search","future":{"x":1}}""")
        val event = assertIs<ResponseStreamEvent.Extension>(ResponseStreamEvent.parse(raw))
        assertEquals("response.web_search_call.searching", event.discriminator)
        assertEquals(7uL, event.sequenceNumber)
        assertEquals(raw, event.raw)
    }

    @Test
    fun `known event missing a required field is not an extension`() {
        listOf(
            """{"type":"response.output_text.delta","sequence_number":1}""",
            """{"type":"response.output_text.delta","sequence_number":-1,"output_index":0,"content_index":0,"item_id":"msg","delta":"x"}""",
            """{"sequence_number":1}""",
        ).forEach { encoded -> assertFails { ResponseStreamEvent.parse(json(encoded)) } }
    }

    @Test
    fun `done sentinel cannot replace or precede a terminal`() {
        val decoder = ResponsesStreamDecoder()
        assertFails { decoder.push("data: [DONE]\n\n".encodeToByteArray()) }
    }

    private fun json(value: String): JsonObject = Json.parseToJsonElement(value).jsonObject
}

package com.garive.eng.kt.anthropic

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFails
import kotlin.test.assertIs
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject

class MessagesEventsTest {
    @Test
    fun `portable event and delta catalogues use exact discriminators`() {
        assertEquals(8, PortableEventKind.entries.size)
        assertEquals(5, DeltaKind.entries.size)
        assertEquals(PortableEventKind.CONTENT_BLOCK_DELTA, PortableEventKind.fromWireName("content_block_delta"))
        assertEquals(DeltaKind.INPUT_JSON, DeltaKind.fromWireName("input_json_delta"))
    }

    @Test
    fun `every portable delta family validates required data`() {
        val deltas = listOf(
            """{"type":"text_delta","text":"你"}""" to DeltaKind.TEXT,
            """{"type":"input_json_delta","partial_json":"{}"}""" to DeltaKind.INPUT_JSON,
            """{"type":"thinking_delta","thinking":"why"}""" to DeltaKind.THINKING,
            """{"type":"signature_delta","signature":"opaque"}""" to DeltaKind.SIGNATURE,
            """{"type":"citations_delta","citation":{"type":"char_location"}}""" to DeltaKind.CITATION,
        )
        deltas.forEachIndexed { index, (delta, kind) ->
            val event = json("""{"type":"content_block_delta","index":$index,"delta":$delta}""")
            val parsed = assertIs<StreamEvent.Portable>(StreamEvent.parse(event))
            assertEquals(kind, parsed.deltaKind)
        }
    }

    @Test
    fun `future event and delta are lossless extensions`() {
        val futureEvent = json("""{"type":"future_event","nested":{"x":1}}""")
        assertEquals(futureEvent, assertIs<StreamEvent.Extension>(StreamEvent.parse(futureEvent)).raw)
        val futureDelta = json("""{"type":"content_block_delta","index":0,"delta":{"type":"future_delta","bytes":"opaque"}}""")
        val parsed = assertIs<StreamEvent.Portable>(StreamEvent.parse(futureDelta))
        assertEquals(null, parsed.deltaKind)
        assertEquals(futureDelta, parsed.raw)
    }

    @Test
    fun `known deltas missing required fields fail instead of becoming extensions`() {
        listOf(
            """{"type":"content_block_delta","index":0,"delta":{"type":"text_delta"}}""",
            """{"type":"content_block_delta","index":-1,"delta":{"type":"text_delta","text":"x"}}""",
            """{"type":"content_block_stop"}""",
        ).forEach { value -> assertFails { StreamEvent.parse(json(value)) } }
    }

    @Test
    fun `event name and lifecycle mismatches fail closed`() {
        val mismatch = "event: ping\ndata: {\"type\":\"message_stop\"}\n\n".encodeToByteArray()
        assertFails { MessagesStreamDecoder().push(mismatch) }
        val beforeStart = "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n".encodeToByteArray()
        assertFails { MessagesStreamDecoder().push(beforeStart) }
    }

    private fun json(value: String): JsonObject = Json.parseToJsonElement(value).jsonObject
}

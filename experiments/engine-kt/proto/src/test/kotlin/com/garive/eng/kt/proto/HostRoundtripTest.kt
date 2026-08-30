package com.garive.eng.kt.proto

import com.garive.host.v1.Host
import kotlin.test.Test
import kotlin.test.assertEquals

class HostRoundtripTest {
    @Test fun `generated Host v1 preserves unsigned position bits and unknown event text`() {
        val event = Host.HostEventV1.newBuilder()
            .setApiVersion("v1")
            .setSessionId("session-1")
            .setPosition(-1L)
            .setEvent("future.unknown")
            .build()
        val decoded = Host.HostEventV1.parseFrom(event.toByteArray())
        assertEquals(event, decoded)
        assertEquals(ULong.MAX_VALUE, decoded.position.toULong())
    }

    @Test fun `generated Host v1 preserves typed continuation JSON presence`() {
        val request = Host.ContinueTurnRequestV1.newBuilder()
            .setSessionId("session-1")
            .setSuspensionId("suspension-1")
            .setExpectedSessionVersion(3)
            .setInputJson("true")
            .build()
        val decoded = Host.ContinueTurnRequestV1.parseFrom(request.toByteArray())
        assertEquals(true, decoded.hasInputJson())
        assertEquals("true", decoded.inputJson)
    }
}

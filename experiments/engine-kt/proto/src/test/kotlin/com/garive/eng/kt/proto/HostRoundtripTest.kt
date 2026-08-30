package com.garive.eng.kt.proto

import com.garive.host.v1.Host
import kotlin.test.Test
import kotlin.test.assertEquals

class HostRoundtripTest {
    @Test fun `generated Host v1 preserves unsigned position bits and unknown event text`() {
        val event = Host.HostEventV1.newBuilder()
            .setApiVersion("garive.host.v1")
            .setSessionId("session-1")
            .setPosition(-1L)
            .setEvent("future.unknown")
            .build()
        val decoded = Host.HostEventV1.parseFrom(event.toByteArray())
        assertEquals(event, decoded)
        assertEquals(ULong.MAX_VALUE, decoded.position.toULong())
    }


    @Test fun `typed continuation JSON round trips`() {
        val request = Host.ContinueTurnRequestV1.newBuilder()
            .setSessionId("session")
            .setSuspensionId("suspension")
            .setExpectedSessionVersion(7)
            .setInputJson("{\"approved\":true}")
            .build()
        assertEquals(request, Host.ContinueTurnRequestV1.parseFrom(request.toByteArray()))
    }
}

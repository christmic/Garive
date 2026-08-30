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

    @Test fun `H2 timeline preserves presence bytes and unsigned position bits`() {
        val suspension = Host.SuspensionViewV1.newBuilder()
            .setSuspensionId("suspension")
            .setSessionVersion(9)
            .setKind("approval_required")
            .setPromptSchema("garive.public-suspension-prompt.v1")
            .setPromptJson(com.google.protobuf.ByteString.copyFromUtf8("{\"schema_version\":1}"))
            .setPromptDigest("digest")
            .setResponseSchemaJson(com.google.protobuf.ByteString.copyFromUtf8("{\"type\":\"boolean\"}"))
            .setResponseSchemaDigest("schema-digest")
            .build()
        val item = Host.TurnTimelineItemV1.newBuilder()
            .setTurnId("turn")
            .setStartedPosition(2)
            .setLatestPosition(-1L)
            .setState("suspended")
            .setUserText("hello")
            .setSuspension(suspension)
            .build()
        val page = Host.TurnTimelinePageV1.newBuilder()
            .setApiVersion("v1")
            .setSessionId("session")
            .addItems(item)
            .setScannedThroughPosition(-1L)
            .setObservedMaxPosition(-1L)
            .build()
        val decoded = Host.TurnTimelinePageV1.parseFrom(page.toByteArray())
        assertEquals(page, decoded)
        assertEquals(ULong.MAX_VALUE, decoded.itemsList.single().latestPosition.toULong())
        assertEquals(false, decoded.itemsList.single().hasCompletionText())
        assertEquals(true, decoded.itemsList.single().hasSuspension())
    }
}

package com.garive.runtime.server.host

import com.garive.host.v1.Host
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith

class EmbeddedServerHostTest {
    @Test fun `executable composition root emits one ordered Host v1 terminal`() {
        val scenario = EmbeddedServerHost.runFixture("hello")
        val decoded = Host.FakeHostScenarioV1.parseFrom(scenario.toByteArray())
        assertEquals(listOf(1L, 2L, 3L, 4L, 5L), decoded.eventsList.map { it.position })
        assertEquals("hello from Garive", decoded.eventsList.filter { it.event == "output.delta" }
            .joinToString("") { it.text })
        assertEquals(listOf("turn.completed"), decoded.eventsList.map { it.event }
            .filter { it.startsWith("turn.") && it != "turn.started" })
        assertFailsWith<IllegalArgumentException> { EmbeddedServerHost.runFixture("other") }
    }
}
